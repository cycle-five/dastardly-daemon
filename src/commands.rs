use crate::{
    Data, Error,
    data::{EnforcementAction, GuildConfig, NotificationMethod, PendingEnforcement, Warning},
    enforcement::EnforcementCheckRequest,
};
use chrono::{Duration, Utc};
use poise::serenity_prelude::{Colour, CreateEmbed, CreateMessage, Mentionable, Timestamp, User};
use poise::{Context, command};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Basic ping command
/// This command is used to check if the bot is responsive.
#[command(prefix_command, slash_command, guild_only)]
pub async fn ping(ctx: Context<'_, Data, Error>) -> Result<(), Error> {
    ctx.say("Pong!").await?;
    Ok(())
}

/// Warn a user for inappropriate behavior
#[allow(clippy::too_many_lines)]
#[command(prefix_command, slash_command, required_permissions = "ADMINISTRATOR")]
pub async fn warn(
    ctx: Context<'_, Data, Error>,
    #[description = "User to warn"] user: User,
    #[description = "Reason for warning"] reason: String,
    #[description = "Notification method (DM or Public)"] notification: Option<String>,
    #[description = "Action to take (mute, ban, kick)"] action: Option<String>,
    #[description = "Duration in minutes for mute/ban, delay for kick"] duration_minutes: Option<u64>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a guild")?;

    // Get guild configuration
    let guild_config = ctx.data().guild_configs.get(&guild_id).map_or_else(
        || GuildConfig {
            guild_id: guild_id.get(),
            ..Default::default()
        },
        |entry| entry.clone(),
    );

    // Determine notification method
    let notification_method = match notification.as_deref() {
        Some("public" | "Public") => NotificationMethod::PublicWithMention,
        Some("dm" | "DM") => NotificationMethod::DirectMessage,
        _ => guild_config.default_notification_method,
    };

    // Determine enforcement action
    let duration = duration_minutes.map(|d| d * 60);
    let enforcement = match action.as_deref() {
        Some("mute" | "Mute") => Some(EnforcementAction::Mute {
            duration,
        }),
        Some("ban" | "Ban") => Some(EnforcementAction::Ban {
            duration
        }),
        Some("kick" | "Kick") => Some(EnforcementAction::Kick {
            delay: duration,
        }),
        _ => guild_config.default_enforcement,
    };

    // Create warning
    let warning_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let warning = Warning {
        id: warning_id.clone(),
        user_id: user.id.get(),
        issuer_id: ctx.author().id.get(),
        guild_id: guild_id.get(),
        reason,
        timestamp: now.clone(),
        notification_method,
        enforcement: enforcement.clone(),
    };

    // Store warning
    ctx.data()
        .warnings
        .insert(warning_id.clone(), warning.clone());

    // Create pending enforcement if applicable
    if let Some(action) = enforcement {
        let enforcement_id = Uuid::new_v4().to_string();
        let execute_at = match &action {
            #[allow(clippy::cast_possible_wrap)]
            EnforcementAction::Ban { duration } | EnforcementAction::Mute { duration } => {
                Utc::now() + Duration::seconds(duration.unwrap_or(0) as i64)
            }
            #[allow(clippy::cast_possible_wrap)]
            EnforcementAction::Kick { delay } => {
                Utc::now() + Duration::seconds(delay.unwrap_or(0) as i64)
            }
            EnforcementAction::None => unreachable!(),
        };

        let pending = PendingEnforcement {
            id: enforcement_id.clone(),
            warning_id,
            user_id: user.id.get(),
            guild_id: guild_id.get(),
            action,
            execute_at: execute_at.to_rfc3339(),
            executed: false,
        };
        ctx.data()
            .pending_enforcements
            .insert(enforcement_id, pending);
    }

    // Notify user based on notification method
    match warning.notification_method {
        NotificationMethod::DirectMessage => {
            if let Ok(channel) = user.create_dm_channel(&ctx.http()).await {
                let embed = CreateEmbed::new()
                    .title("Warning Received")
                    .description(format!(
                        "You have been warned in {} for: {}",
                        ctx.guild().unwrap().name,
                        warning.reason
                    ))
                    .colour(Colour::RED)
                    .timestamp(Timestamp::now());

                let message = CreateMessage::new().embed(embed);
                let _ = channel.send_message(&ctx.http(), message).await;
            }
        }
        NotificationMethod::PublicWithMention => {
            let content = format!(
                "{} You have been warned for: {}",
                user.mention(),
                warning.reason
            );
            let embed = CreateEmbed::new()
                .title("Warning Issued")
                .description(&content)
                .colour(Colour::RED)
                .timestamp(Timestamp::now());

            ctx.send(poise::CreateReply::default().embed(embed)).await?;
        }
    }

    // Log the warning
    info!(
        target: crate::COMMAND_TARGET,
        command = "warn",
        guild_id = %guild_id.get(),
        user_id = %user.id.get(),
        issuer_id = %ctx.author().id.get(),
        reason = %warning.reason,
        event = "warning_issued",
        "Warning issued to user"
    );

    // Save data
    if let Err(e) = ctx.data().save().await {
        error!("Failed to save data after warning: {}", e);
    }

    info!(
        target: crate::COMMAND_TARGET,
        command = "warn",
        guild_id = %guild_id.get(),
        user_id = %user.id.get(),
        issuer_id = %ctx.author().id.get(),
        reason = %warning.reason,
        event = "warning_saved",
        "Warning saved to database"
    );

    // If there's an immediate action, notify the enforcement task
    if let Some(action) = &warning.enforcement {
        match action {
            EnforcementAction::Kick { delay } if delay.is_some_and(|d| d == 0) || delay.is_none() => {
                // For immediate kicks, notify the enforcement task
                if let Some(tx) = &ctx.data().enforcement_tx {
                    let _ = tx
                        .send(EnforcementCheckRequest::CheckUser {
                            user_id: user.id.get(),
                            guild_id: guild_id.get(),
                        })
                        .await;
                }
            }
            _ => {
                warn!("Enforcement action is not immediate: {action:?}");
            } // Other actions will be handled by the regular check interval
        }
    }

    ctx.say(format!("Warned {} for: {}", user.name, warning.reason))
        .await?;
    Ok(())
}

/// Cancel a pending enforcement action
#[command(
    prefix_command,
    slash_command,
    guild_only,
    required_permissions = "ADMINISTRATOR"
)]
pub async fn cancelwarning(
    ctx: Context<'_, Data, Error>,
    #[description = "User whose enforcement to cancel"] user: User,
    #[description = "Specific enforcement ID to cancel (optional)"] enforcement_id: Option<String>,
) -> Result<(), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a guild")?;
    let user_id = user.id.get();
    let mut canceled = false;
    let mut response = String::new();

    // Find pending enforcements for this user in this guild
    let mut pending_to_cancel = Vec::new();
    for entry in &ctx.data().pending_enforcements {
        let pending = entry.value();
        if pending.user_id == user_id && pending.guild_id == guild_id.get() && !pending.executed {
            if let Some(ref eid) = enforcement_id {
                if pending.id == *eid {
                    pending_to_cancel.push(pending.id.clone());
                    break;
                }
            } else {
                pending_to_cancel.push(pending.id.clone());
            }
        }
    }

    // Cancel the found enforcements
    for id in pending_to_cancel {
        if let Some(mut pending) = ctx.data().pending_enforcements.get_mut(&id) {
            pending.executed = true;
            canceled = true;
            #[allow(clippy::format_push_string)]
            response.push_str(&format!(
                "Canceled enforcement ID {} for {}\n",
                id, user.name
            ));

            // Notify the enforcement task that this enforcement has been canceled
            if let Some(tx) = &ctx.data().enforcement_tx {
                let _ = tx
                    .send(EnforcementCheckRequest::CheckEnforcement {
                        enforcement_id: id.clone(),
                    })
                    .await;
            }
        }
    }

    if !canceled {
        response = format!("No pending enforcements found for {}", user.name);
    }

    // Save data
    if canceled {
        if let Err(e) = ctx.data().save().await {
            error!("Failed to save data after canceling warning: {}", e);
        }
    }

    ctx.say(response).await?;
    Ok(())
}

// // Admin check function for commands that require admin permissions
// async fn admin_check(ctx: Context<'_, Data, Error>) -> Result<bool, Error> {
//     // let guild = match ctx
//     //     .guild() {
//     //     Some(guild) => guild,
//     //     None => {
//     //         ctx.say("This command can only be used in a server").await?;
//     //         return Ok(false);
//     //     }
//     // }.clone();

//     if let Some(member) = ctx.author_member().await {
//         #[allow(deprecated)]
//         //let permissions = guild.member_permissions(&member);
//         let permissions = member.permissions(ctx)?;
//         return Ok(permissions.administrator() || permissions.manage_guild());
//     }
//     ctx.say("This command can only be used by administrators")
//         .await?;
//     Ok(false)
// }

#[cfg(test)]
mod tests {
    use super::*;

    // Test that the ping command is properly defined
    #[test]
    fn test_ping_command_definition() {
        let cmd = ping();
        assert_eq!(cmd.name, "ping");
        assert!(
            cmd.description
                .unwrap_or_else(Default::default)
                .contains("check if the bot is responsive")
        );
        assert!(cmd.guild_only);
    }

    // This test verifies that the ping command can be executed
    #[test]
    fn test_ping_command_can_be_called() {
        // This test just verifies that the ping command exists and can be called
        // We don't actually execute it since that would require a real Discord context
        let cmd = ping();
        assert!(cmd.create_as_slash_command().is_some());
    }

    #[test]
    fn test_warn_command_definition() {
        let cmd = warn();
        assert_eq!(cmd.name, "warn");
        assert!(cmd.guild_only);
        assert!(cmd.checks.len() == 1);
    }

    #[test]
    fn test_cancelwarning_command_definition() {
        let cmd = cancelwarning();
        assert_eq!(cmd.name, "cancelwarning");
        assert!(cmd.guild_only);
        assert!(cmd.checks.len() == 1);
    }
}
