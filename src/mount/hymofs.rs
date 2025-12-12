use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use log::{debug, warn};
use walkdir::WalkDir;

const HYMO_CTL: &str = "/proc/hymo_ctl";
const EXPECTED_PROTOCOL_VERSION: i32 = 3;

#[derive(Debug)]
pub enum HymoRule {
    Redirect {
        src: PathBuf,
        target: PathBuf,
        file_type: u8,
    },
    Hide {
        path: PathBuf,
    },
    Inject {
        dir: PathBuf,
    },
}

struct HymoDriver {
    ctl_path: PathBuf,
}

impl HymoDriver {
    fn new() -> Result<Self> {
        let path = PathBuf::from(HYMO_CTL);
        if !path.exists() {
            anyhow::bail!("HymoFS control file not found");
        }
        Ok(Self { ctl_path: path })
    }

    fn get_version(&self) -> Option<i32> {
        if let Ok(content) = fs::read_to_string(&self.ctl_path) {
            if let Some(line) = content.lines().next() {
                if let Some(ver_str) = line.strip_prefix("HymoFS Protocol: ") {
                    return ver_str.trim().parse::<i32>().ok();
                }
            }
        }
        None
    }

    fn clear(&self) -> Result<()> {
        let mut file = File::create(&self.ctl_path)?;
        writeln!(file, "clear")?;
        Ok(())
    }

    fn apply_rules(&self, rules: &[HymoRule]) -> Result<()> {
        if rules.is_empty() {
            return Ok(());
        }

        let file = OpenOptions::new().write(true).open(&self.ctl_path)?;
        let mut writer = BufWriter::with_capacity(8192, file);

        for rule in rules {
            match rule {
                HymoRule::Redirect { src, target, file_type } => {
                    writeln!(
                        writer,
                        "add {} {} {}",
                        src.display(),
                        target.display(),
                        file_type
                    )?;
                }
                HymoRule::Hide { path } => {
                    writeln!(writer, "hide {}", path.display())?;
                }
                HymoRule::Inject { dir } => {
                    writeln!(writer, "inject {}", dir.display())?;
                }
            }
        }

        writer.flush()?;
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum HymoFsStatus {
    Available,
    NotPresent,
    KernelTooOld,
    ModuleTooOld,
}

pub struct HymoFs;

impl HymoFs {
    pub fn check_status() -> HymoFsStatus {
        let driver = match HymoDriver::new() {
            Ok(d) => d,
            Err(_) => return HymoFsStatus::NotPresent,
        };

        let version = match driver.get_version() {
            Some(v) => v,
            None => return HymoFsStatus::NotPresent,
        };

        if version != EXPECTED_PROTOCOL_VERSION {
            warn!(
                "HymoFS protocol mismatch! Kernel: {}, User: {}",
                version, EXPECTED_PROTOCOL_VERSION
            );
            if version < EXPECTED_PROTOCOL_VERSION {
                return HymoFsStatus::KernelTooOld;
            } else {
                return HymoFsStatus::ModuleTooOld;
            }
        }

        HymoFsStatus::Available
    }

    pub fn is_available() -> bool {
        Self::check_status() == HymoFsStatus::Available
    }

    pub fn clear() -> Result<()> {
        let driver = HymoDriver::new()?;
        driver.clear()
    }

    pub fn inject_directory(target_base: &Path, module_dir: &Path) -> Result<()> {
        if !module_dir.exists() || !module_dir.is_dir() {
            return Ok(());
        }

        let driver = HymoDriver::new()?;
        let mut rules = Vec::new();

        rules.push(HymoRule::Inject {
            dir: target_base.to_path_buf(),
        });

        for entry in WalkDir::new(module_dir).min_depth(1) {
            let entry = entry?;
            let relative_path = entry.path().strip_prefix(module_dir)?;
            let target_path = target_base.join(relative_path);
            let file_type = entry.file_type();

            if file_type.is_char_device() {
                let metadata = entry.metadata()?;
                if metadata.rdev() == 0 {
                    rules.push(HymoRule::Hide {
                        path: target_path,
                    });
                }
            } else if file_type.is_dir() {
                rules.push(HymoRule::Inject {
                    dir: target_path.clone(),
                });
                
                rules.push(HymoRule::Redirect {
                    src: target_path,
                    target: entry.path().to_path_buf(),
                    file_type: 4, 
                });
            } else {
                let type_code = if file_type.is_symlink() {
                    10 
                } else {
                    8 
                };

                rules.push(HymoRule::Redirect {
                    src: target_path,
                    target: entry.path().to_path_buf(),
                    file_type: type_code,
                });
            }
        }

        driver.apply_rules(&rules).context("Failed to apply HymoFS rules")?;
        
        debug!("Injected {} rules for {}", rules.len(), target_base.display());
        Ok(())
    }

    #[allow(dead_code)]
    pub fn hide_path(path: &Path) -> Result<()> {
        let driver = HymoDriver::new()?;
        let rules = vec![HymoRule::Hide {
            path: path.to_path_buf(),
        }];
        driver.apply_rules(&rules)
    }
}
