use std::{fs::create_dir_all, os::unix::fs::MetadataExt};

use anyhow::{Context, Result, bail};
use rustix::{
    fs::{Gid, Mode, Uid, chmod, chown},
    mount::{MountFlags, MountPropagationFlags, mount_change, mount_move, mount_remount},
};

use crate::{
    magic_mount::{MagicMount, node::NodeFileType},
    utils::{lgetfilecon, lsetfilecon},
};

#[cfg(any(target_os = "linux", target_os = "android"))]
use crate::magic_mount::try_umount::send_unmountable;

impl MagicMount {
    pub fn check_tmpfs(&mut self) {
        for it in &mut self.node.children {
            let (name, node) = it;
            let real_path = self.path.join(name);
            let need = match node.file_type {
                NodeFileType::Symlink => true,
                NodeFileType::Whiteout => real_path.exists(),
                _ => {
                    if let Ok(metadata) = real_path.symlink_metadata() {
                        let file_type = NodeFileType::from(metadata.file_type());
                        file_type != node.file_type || file_type == NodeFileType::Symlink
                    } else {
                        // real path not exists
                        true
                    }
                }
            };
            if need {
                if node.module_path.is_none() {
                    log::error!(
                        "cannot create tmpfs on {}, ignore: {name}",
                        self.path.display()
                    );
                    node.skip = true;
                    continue;
                }
                self.has_tmpfs = true;
                break;
            }
        }
    }

    pub fn moving_tmpfs(&self) -> Result<()> {
        log::debug!(
            "moving tmpfs {} -> {}",
            self.work_dir_path.display(),
            self.path.display()
        );

        if let Err(e) = mount_remount(
            &self.work_dir_path,
            MountFlags::RDONLY | MountFlags::BIND,
            "",
        ) {
            log::warn!("make dir {} ro: {e:#?}", self.path.display());
        }
        mount_move(&self.work_dir_path, &self.path)
            .context("move self")
            .with_context(|| {
                format!(
                    "moving tmpfs {} -> {}",
                    self.work_dir_path.display(),
                    self.path.display()
                )
            })?;
        // make private to reduce peer group count
        if let Err(e) = mount_change(&self.path, MountPropagationFlags::PRIVATE) {
            log::warn!("make dir {} private: {e:#?}", self.path.display());
        }

        #[cfg(any(target_os = "linux", target_os = "android"))]
        if self.umount {
            // tell ksu about this one too
            let _ = send_unmountable(&self.path);
        }

        Ok(())
    }

    pub fn creatimg_tmpfs_skeleton(&self) -> Result<()> {
        log::debug!(
            "creating tmpfs skeleton for {} at {}",
            self.path.display(),
            self.work_dir_path.display()
        );

        let _ = create_dir_all(&self.work_dir_path);

        let (metadata, path) = {
            if self.path.exists() {
                (self.path.metadata()?, &self.path)
            } else if let Some(module_path) = &self.node.module_path {
                (module_path.metadata()?, module_path)
            } else {
                bail!("cannot mount root dir {}!", self.path.display());
            }
        };

        chmod(&self.work_dir_path, Mode::from_raw_mode(metadata.mode()))?;
        chown(
            &self.work_dir_path,
            Some(Uid::from_raw(metadata.uid())),
            Some(Gid::from_raw(metadata.gid())),
        )?;
        lsetfilecon(&self.work_dir_path, lgetfilecon(path)?.as_str())?;

        Ok(())
    }
}
