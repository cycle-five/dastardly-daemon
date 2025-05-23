use chrono::Utc;
use dastardly_daemon::data::{Data, EnforcementState, PendingEnforcement};
use dastardly_daemon::enforcement_new::EnforcementAction;
use std::time::Duration;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    println!("Enforcement Lifecycle Test");
    println!("-------------------------");

    // Create a new data instance
    let data = Data::new();

    // Create a test user and guild
    let user_id = 12345;
    let guild_id = 67890;

    // 1. Create a pending enforcement with a duration (will need reversal)
    let enforcement_id_1 = Uuid::new_v4().to_string();
    let warning_id_1 = Uuid::new_v4().to_string();
    let now = Utc::now();
    let execute_at = Utc::now() + chrono::Duration::seconds(1);
    let reverse_at = Utc::now() + chrono::Duration::seconds(3);

    let enforcement1 = PendingEnforcement {
        id: enforcement_id_1.clone(),
        warning_id: warning_id_1,
        user_id,
        guild_id,
        action: EnforcementAction::voice_mute(2),
        execute_at,
        reverse_at: Some(reverse_at.clone()),
        state: EnforcementState::Pending,
        created_at: now.clone(),
        executed_at: None,
        reversed_at: None,
        executed: false,
    };

    // 2. Create a one-time enforcement (no reversal needed)
    let enforcement_id_2 = Uuid::new_v4().to_string();
    let warning_id_2 = Uuid::new_v4().to_string();

    let enforcement2 = PendingEnforcement {
        id: enforcement_id_2.clone(),
        warning_id: warning_id_2,
        user_id,
        guild_id,
        action: EnforcementAction::voice_disconnect(0),
        execute_at: now.clone(),
        reverse_at: None,
        state: EnforcementState::Pending,
        created_at: now.clone(),
        executed_at: None,
        reversed_at: None,
        executed: false,
    };

    // Add enforcements to the pending map
    data.pending_enforcements
        .insert(enforcement_id_1.clone(), enforcement1);
    data.pending_enforcements
        .insert(enforcement_id_2.clone(), enforcement2);

    // Verify initial state
    println!("\n--- Initial State ---");
    println!("Pending enforcements: {}", data.pending_enforcements.len());
    println!("Active enforcements: {}", data.active_enforcements.len());
    println!(
        "Completed enforcements: {}",
        data.completed_enforcements.len()
    );

    // Simulate enforcement execution (1st enforcement)
    println!("\n--- Simulating Execution of 1st Enforcement (needs reversal) ---");
    if let Some(mut pending) = data.pending_enforcements.get_mut(&enforcement_id_1) {
        println!("Found enforcement with id: {}", pending.id);
        pending.state = EnforcementState::Active;
        pending.executed_at = Some(now.clone());
        pending.executed = true;

        // Clone it and move it to active
        let enforcement_data = pending.value().clone();
        drop(pending);

        data.pending_enforcements.remove(&enforcement_id_1);
        data.active_enforcements
            .insert(enforcement_id_1.clone(), enforcement_data);

        println!("Moved to active enforcements");
    }

    // Simulate enforcement execution (2nd enforcement - one-time action)
    println!("\n--- Simulating Execution of 2nd Enforcement (one-time action) ---");
    if let Some(mut pending) = data.pending_enforcements.get_mut(&enforcement_id_2) {
        println!("Found enforcement with id: {}", pending.id);
        pending.state = EnforcementState::Completed; // One-time actions go directly to completed
        pending.executed_at = Some(now.clone());
        pending.executed = true;

        // Clone it and move it to completed
        let enforcement_data = pending.value().clone();
        drop(pending);

        data.pending_enforcements.remove(&enforcement_id_2);
        data.completed_enforcements
            .insert(enforcement_id_2.clone(), enforcement_data);

        println!("Moved directly to completed enforcements (one-time action)");
    }

    // Verify state after execution
    println!("\n--- State After Execution ---");
    println!("Pending enforcements: {}", data.pending_enforcements.len());
    println!("Active enforcements: {}", data.active_enforcements.len());
    println!(
        "Completed enforcements: {}",
        data.completed_enforcements.len()
    );

    // Simulate time passing for the reversal
    println!("\n--- Sleeping to simulate passage of time for reversal ---");
    std::thread::sleep(Duration::from_secs(3));

    // Simulate enforcement reversal
    println!("\n--- Simulating Reversal of 1st Enforcement ---");
    if let Some(mut active) = data.active_enforcements.get_mut(&enforcement_id_1) {
        println!("Found active enforcement with id: {}", active.id);
        active.state = EnforcementState::Reversed;
        active.reversed_at = Some(Utc::now());

        // Clone it and move it to completed
        let enforcement_data = active.value().clone();
        drop(active);

        data.active_enforcements.remove(&enforcement_id_1);
        data.completed_enforcements
            .insert(enforcement_id_1.clone(), enforcement_data);

        println!("Moved to completed enforcements after reversal");
    }

    // Verify final state
    println!("\n--- Final State ---");
    println!("Pending enforcements: {}", data.pending_enforcements.len());
    println!("Active enforcements: {}", data.active_enforcements.len());
    println!(
        "Completed enforcements: {}",
        data.completed_enforcements.len()
    );

    // Examine completed enforcements
    println!("\n--- Completed Enforcements ---");
    for entry in &data.completed_enforcements {
        let enforcement = entry.value();
        println!("ID: {}", enforcement.id);
        println!("State: {:?}", enforcement.state);
        println!("Action: {:?}", enforcement.action);
        println!("Created at: {}", enforcement.created_at);
        println!("Executed at: {:?}", enforcement.executed_at);
        println!("Reversed at: {:?}", enforcement.reversed_at);
        println!("---");
    }

    println!("\nEnforcement lifecycle test completed successfully!");
}
