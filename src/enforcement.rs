use crate::{Data, Error, data::EnforcementAction};
use poise::serenity_prelude::{
    self as serenity, 
    Context, 
    GuildId, 
    UserId, 
    Member,
    Guild,
    PermissionOverwrite,
    PermissionOverwriteType,
    Permissions
};
use chrono::{Utc, DateTime};
use tracing::{info, error};
use std::time::Duration;
use std::time::Duration as StdDuration;

/// Check and execute pending enforcements
pub async fn check_and_execute_enforcements(ctx: &Context, data: &Data) -> Result<(), Error> {
    let now = Utc::now();
    let mut enforcements_to_execute = Vec::new();
    
    // Find enforcements that need to be executed
    for entry in data.pending_enforcements.iter() {
        let pending = entry.value();
        if !pending.executed {
            let execute_at = DateTime::parse_from_rfc3339(&pending.execute_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| DateTime::<Utc>::MIN_UTC);
                
            if execute_at <= now {
                enforcements_to_execute.push(pending.id.clone());
            }
        }
    }
    
    // Execute each enforcement - use a clone to avoid borrowing issues
    let enforcements_to_execute_clone = enforcements_to_execute.clone();
    for id in enforcements_to_execute_clone {
        if let Some(mut pending) = data.pending_enforcements.get_mut(&id) {
            let guild_id = GuildId::new(pending.guild_id);
            let user_id = UserId::new(pending.user_id);
            
            // Execute the action based on the type
            match &pending.action {
                EnforcementAction::Mute { duration } => {
                    // Unmute the user
                    info!("Unmuting user {} in guild {}", user_id, guild_id);
                    if let Ok(guild) = guild_id.to_partial_guild(&ctx).await {
                        if let Ok(mut member) = guild.member(&ctx, user_id).await {
                            let until = Utc::now() + Duration::from_secs(*duration);
                            if let Err(e) = member.disable_communication_until_datetime(&ctx, until.into()).await {
                                error!("Failed to unmute user {}: {}", user_id, e);
                            } else {
                                info!("Successfully unmuted user {}", user_id);
                            }
                        }
                    }
                },
                EnforcementAction::Ban { duration } => {
                    // Unban the user
                    info!("Banning user {} in guild {}", user_id, guild_id);
                    let dmd = if *duration > 7 { 7 as u8 } else { *duration as u8 };
                    if let Err(e) = guild_id.ban(&ctx, user_id, dmd).await {
                        error!("Failed to unban user {}: {}", user_id, e);
                    } else {
                        info!("Successfully unbanned user {}", user_id);
                    }
                },
                EnforcementAction::DelayedKick { delay: _ } => {
                    // Kick the user
                    info!("Kicking user {} from guild {}", user_id, guild_id);
                    if let Ok(guild) = guild_id.to_partial_guild(&ctx).await {
                        if let Ok(member) = guild.member(&ctx, user_id).await {
                            if let Err(e) = member.kick(&ctx).await {
                                error!("Failed to kick user {}: {}", user_id, e);
                            } else {
                                info!("Successfully kicked user {}", user_id);
                            }
                        }
                    }
                },
                EnforcementAction::None => {}
            }
            
            pending.executed = true;
            info!(
                target: crate::COMMAND_TARGET,
                enforcement_id = %id,
                user_id = %pending.user_id,
                guild_id = %pending.guild_id,
                event = "enforcement_executed",
                "Enforcement action executed"
            );
        }
    }
    
    // // Save updated data
    // if !enforcements_to_execute.is_empty() {
    //     if let Err(e) = data.save().await {
    //         error!("Failed to save data after executing enforcements: {}", e);
    //     }
    // }
    
    Ok(())
}

// /// Schedule a timed check for enforcements
// pub fn schedule_next_check(ctx: &Context, data: &Data) {
//     let ctx_clone = ctx.clone();
//     let data_clone = data.clone();
    
//     // Check for enforcements every 30 seconds
//     let _ = ctx.shard.lock().schedule_event(StdDuration::from_secs(30), move || {
//         Box::pin(async move {
//             if let Err(e) = check_and_execute_enforcements(&ctx_clone, &data_clone).await {
//                 error!("Error checking enforcements: {}", e);
//             }
//             schedule_next_check(&ctx_clone, &data_clone);
//         })
//     });
// }
