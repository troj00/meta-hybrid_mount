// Copyright 2025 Meta-Hybrid Mount Authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    ffi::CString,
    fs::read_dir,
    os::fd::RawFd,
    path::Path,
    sync::{
        LazyLock, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result, bail};
use ksu::TryUmount;
use nix::ioctl_write_ptr_bad;

use crate::defs::{DISABLE_FILE_NAME, REMOVE_FILE_NAME, SKIP_MOUNT_FILE_NAME};

const KSU_INSTALL_MAGIC1: u32 = 0xDEADBEEF;
const KSU_INSTALL_MAGIC2: u32 = 0xCAFEBABE;
const KSU_IOCTL_NUKE_EXT4_SYSFS: u32 = 0x40004b11;

static DRIVER_FD: OnceLock<RawFd> = OnceLock::new();
static LAST: AtomicBool = AtomicBool::new(false);
pub static TMPFS: OnceLock<String> = OnceLock::new();
pub static LIST: LazyLock<Mutex<TryUmount>> = LazyLock::new(|| Mutex::new(TryUmount::new()));

#[repr(C)]
struct NukeExt4SysfsCmd {
    arg: u64,
}

ioctl_write_ptr_bad!(
    ksu_nuke_ext4_sysfs,
    KSU_IOCTL_NUKE_EXT4_SYSFS,
    NukeExt4SysfsCmd
);

fn grab_fd() -> i32 {
    let mut fd = -1;

    unsafe {
        libc::syscall(
            libc::SYS_reboot,
            KSU_INSTALL_MAGIC1,
            KSU_INSTALL_MAGIC2,
            0,
            &mut fd,
        );
    };

    fd
}

pub fn send_unmountable<P>(target: P) -> Result<()>
where
    P: AsRef<Path>,
{
    if LAST.load(Ordering::Relaxed) {
        return Ok(());
    }

    for entry in read_dir("/data/adb/modules")?.flatten() {
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        if !path.join("module.prop").exists() {
            continue;
        }

        let disabled =
            path.join(DISABLE_FILE_NAME).exists() || path.join(REMOVE_FILE_NAME).exists();
        let skip = path.join(SKIP_MOUNT_FILE_NAME).exists();
        if disabled || skip {
            continue;
        }

        if !path.ends_with("zygisksu") {
            continue;
        }

        if crate::utils::check_zygisksu_enforce_status()
            && TMPFS.get().is_some_and(|s| s.trim() == "/debug_ramdisk")
        {
            log::warn!("ZygiskSU/ZN detected, canceling try_umount.");
            LAST.store(true, Ordering::Relaxed);
            return Ok(());
        }
    }

    LIST.lock().unwrap().add(target);

    Ok(())
}

pub fn commit() -> Result<()> {
    let mut list = LIST.lock().unwrap();
    list.flags(2);
    list.umount()?;
    Ok(())
}

pub fn ksu_nuke_sysfs(target: &str) -> Result<()> {
    let c_path = CString::new(target)?;

    let cmd = NukeExt4SysfsCmd {
        arg: c_path.as_ptr() as u64,
    };

    let fd = *DRIVER_FD.get_or_init(grab_fd);

    if fd < 0 {
        bail!("KSU driver not available");
    }

    unsafe {
        ksu_nuke_ext4_sysfs(fd, &cmd).context("KSU Nuke Sysfs ioctl failed")?;
    }

    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
pub fn ksu_nuke_sysfs(_target: &str) -> Result<()> {
    bail!("Not supported on this OS")
}
