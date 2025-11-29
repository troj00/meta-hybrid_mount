// meta-hybrid_mount/src/executor.rs
use std::path::PathBuf;
use anyhow::Result;
use crate::{config, magic_mount, overlay_mount, utils};
use crate::planner::MountPlan;

pub fn execute(plan: &MountPlan, config: &config::Config) -> Result<()> {
    // Phase A: OverlayFS
    for op in &plan.overlay_ops {
        let layer_paths: Vec<String> = op.layers.iter()
            .map(|p| p.display().to_string())
            .collect();
            
        log::info!("Mounting {} [OVERLAY] ({} layers)", op.target, layer_paths.len());
        
        if let Err(e) = overlay_mount::mount_overlay(&op.target, &layer_paths, None, None, config.disable_umount) {
            log::error!("OverlayFS mount failed for {}: {:#}. (Magic fallback not implemented in this phase yet)", op.target, e);
            // In a future advanced planner, we could dynamically fallback, 
            // but the planner has already decided the strategy here.
        }
    }

    // Phase B: Magic Mount
    if !plan.magic_module_paths.is_empty() {
        let tempdir = if let Some(t) = &config.tempdir { 
            t.clone() 
        } else { 
            utils::select_temp_dir()? 
        };
        
        log::info!("Starting Magic Mount Engine for {} modules...", plan.magic_module_paths.len());
        
        utils::ensure_temp_dir(&tempdir)?;
        
        if let Err(e) = magic_mount::mount_partitions(
            &tempdir, 
            &plan.magic_module_paths, 
            &config.mountsource, 
            &config.partitions, 
            config.disable_umount
        ) {
            log::error!("Magic Mount failed: {:#}", e);
        }
        
        utils::cleanup_temp_dir(&tempdir);
    }

    Ok(())
}
