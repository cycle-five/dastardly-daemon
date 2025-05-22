# Enforcement System Refactoring Plan

## Current Issues

After analyzing the enforcement system in the Dastardly Daemon bot, I've identified several issues that need to be addressed:

1. **Complex State Management**: 
   - The enforcement system uses three separate `DashMap` collections for different states (pending, active, completed)
   - Manual state transitions with code duplication
   - State tracking is spread across multiple modules

2. **Code Duplication**:
   - Similar patterns in `check_all_enforcements`, `check_user_enforcements`, and `check_specific_enforcement`
   - Duplicated validation logic
   - Similar handling for different action types
   - Duplicate code between `execute_enforcement` and `reverse_enforcement`

3. **Complex Action Type Handling**:
   - Different action handlers for each action type (mute, ban, kick, etc.)
   - Duplicate parameter extraction and error handling
   - Inconsistent parameter validation

4. **Error Handling**:
   - Uses generic `Box<dyn Error>` for errors
   - Error messages aren't structured for better debugging
   - Missing validation in some paths

5. **Legacy Compatibility**:
   - Dual state tracking with both state enum and boolean flag
   - Outdated code paths that are commented out

## Refactoring Plan

### 1. Enforcement Action Enum Refactoring

The current `EnforcementAction` enum has different variants with inconsistent parameters. We'll reorganize it to be more consistent:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnforcementAction {
    /// No action (placeholder)
    None,
    
    /// Text channel timeout
    Mute(ActionParams),
    
    /// Server ban
    Ban(ActionParams),
    
    /// Server kick
    Kick(ActionParams),
    
    /// Voice mute
    VoiceMute(ActionParams),
    
    /// Voice deafen
    VoiceDeafen(ActionParams),
    
    /// Voice disconnect
    VoiceDisconnect(ActionParams),
    
    /// Voice channel haunting (teleportation)
    VoiceChannelHaunt(HauntParams),
}

/// Parameters common to most enforcement actions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActionParams {
    /// Duration in seconds for timed actions, or delay for immediate actions
    pub duration: Option<u64>,
    
    /// Reason for the action (for audit logs)
    pub reason: Option<String>,
}

/// Parameters specific to the VoiceChannelHaunt action
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HauntParams {
    /// Number of times to teleport the user between channels
    pub teleport_count: Option<u64>,
    
    /// Seconds between each teleport
    pub interval: Option<u64>,
    
    /// Whether to eventually return the user to their original channel
    pub return_to_origin: Option<bool>,
    
    /// Original voice channel ID to potentially return to
    pub original_channel_id: Option<u64>,
}
```

### 2. Unified Enforcement Store

Rather than having three separate `DashMap`s, we'll use a single store with state tracking in the object:

```rust
/// Enforcement store that manages all enforcements
pub struct EnforcementStore {
    /// Single map containing all enforcements
    enforcements: DashMap<String, EnforcementRecord>,
}

impl EnforcementStore {
    /// Create a new enforcement store
    pub fn new() -> Self {
        Self {
            enforcements: DashMap::new(),
        }
    }
    
    /// Add a new enforcement
    pub fn add(&self, record: EnforcementRecord) {
        self.enforcements.insert(record.id.clone(), record);
    }
    
    /// Get pending enforcements due for execution
    pub fn get_pending_for_execution(&self) -> Vec<String> {
        let now = Utc::now();
        self.enforcements
            .iter()
            .filter_map(|entry| {
                let record = entry.value();
                if record.state == EnforcementState::Pending && record.execute_at <= now {
                    Some(record.id.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    
    /// Get active enforcements due for reversal
    pub fn get_active_for_reversal(&self) -> Vec<String> {
        let now = Utc::now();
        self.enforcements
            .iter()
            .filter_map(|entry| {
                let record = entry.value();
                if record.state == EnforcementState::Active 
                   && record.reverse_at.is_some() 
                   && record.reverse_at.unwrap() <= now {
                    Some(record.id.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    
    // Additional methods for filtering and querying records
}
```

### 3. Enforcement State Machine

Create a proper state machine for enforcement lifecycle:

```rust
impl EnforcementRecord {
    /// Execute a pending enforcement, transitioning to Active or Completed
    pub fn execute(&mut self) -> Result<(), EnforcementError> {
        if self.state != EnforcementState::Pending {
            return Err(EnforcementError::InvalidStateTransition);
        }
        
        self.state = if self.needs_reversal() {
            EnforcementState::Active
        } else {
            EnforcementState::Completed
        };
        
        self.executed_at = Some(Utc::now());
        self.executed = true; // For backward compatibility
        
        Ok(())
    }
    
    /// Reverse an active enforcement, transitioning to Reversed
    pub fn reverse(&mut self) -> Result<(), EnforcementError> {
        if self.state != EnforcementState::Active {
            return Err(EnforcementError::InvalidStateTransition);
        }
        
        self.state = EnforcementState::Reversed;
        self.reversed_at = Some(Utc::now());
        
        Ok(())
    }
    
    /// Cancel a pending or active enforcement, transitioning to Cancelled
    pub fn cancel(&mut self) -> Result<(), EnforcementError> {
        if self.state != EnforcementState::Pending && self.state != EnforcementState::Active {
            return Err(EnforcementError::InvalidStateTransition);
        }
        
        self.state = EnforcementState::Cancelled;
        
        Ok(())
    }
    
    /// Check if this enforcement needs reversal
    pub fn needs_reversal(&self) -> bool {
        // One-time actions don't need reversal
        !matches!(
            self.action,
            EnforcementAction::Kick(_) 
                | EnforcementAction::VoiceDisconnect(_) 
                | EnforcementAction::VoiceChannelHaunt(_)
                | EnforcementAction::None
        ) && self.has_duration()
    }
    
    /// Check if this enforcement has a duration
    pub fn has_duration(&self) -> bool {
        match &self.action {
            EnforcementAction::Mute(params) |
            EnforcementAction::Ban(params) |
            EnforcementAction::VoiceMute(params) |
            EnforcementAction::VoiceDeafen(params) => params.duration.is_some() && params.duration.unwrap() > 0,
            _ => false,
        }
    }
    
    // Other utility methods
}
```

### 4. Action Handlers

Implement a trait-based system for action handlers to avoid large match statements:

```rust
/// Trait for enforcement action handlers
pub trait ActionHandler: Send + Sync {
    /// Execute the action
    async fn execute(&self, 
                    http: &Http, 
                    guild_id: GuildId, 
                    user_id: UserId, 
                    params: &dyn Any) -> Result<(), EnforcementError>;
    
    /// Reverse the action
    async fn reverse(&self, 
                    http: &Http, 
                    guild_id: GuildId, 
                    user_id: UserId, 
                    params: &dyn Any) -> Result<(), EnforcementError>;
    
    /// Check if the action is immediate or requires scheduling
    fn is_immediate(&self, params: &dyn Any) -> bool;
}

/// Registry of action handlers
pub struct ActionHandlerRegistry {
    handlers: HashMap<EnforcementActionType, Box<dyn ActionHandler>>,
}

impl ActionHandlerRegistry {
    /// Create a new registry with all handlers registered
    pub fn new() -> Self {
        let mut handlers = HashMap::new();
        handlers.insert(EnforcementActionType::Mute, Box::new(MuteActionHandler));
        handlers.insert(EnforcementActionType::Ban, Box::new(BanActionHandler));
        // Register other handlers
        
        Self { handlers }
    }
    
    /// Get a handler for a specific action type
    pub fn get_handler(&self, action_type: EnforcementActionType) -> Option<&dyn ActionHandler> {
        self.handlers.get(&action_type).map(|h| h.as_ref())
    }
}
```

### 5. Error Handling

Create a dedicated error type for enforcement operations:

```rust
/// Errors that can occur during enforcement operations
#[derive(Debug, Error)]
pub enum EnforcementError {
    #[error("Invalid state transition")]
    InvalidStateTransition,
    
    #[error("Enforcement not found: {0}")]
    NotFound(String),
    
    #[error("Discord API error: {0}")]
    DiscordApi(#[from] serenity::Error),
    
    #[error("Failed to get guild or member: {0}")]
    GuildOrMemberNotFound(String),
    
    #[error("Action validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("User not in voice channel")]
    NotInVoiceChannel,
    
    #[error("Other error: {0}")]
    Other(String),
}
```

### 6. Unified Enforcement Service

Create a service that handles all enforcement operations:

```rust
/// Service for enforcement operations
pub struct EnforcementService {
    store: EnforcementStore,
    handlers: ActionHandlerRegistry,
}

impl EnforcementService {
    /// Create a new enforcement service
    pub fn new() -> Self {
        Self {
            store: EnforcementStore::new(),
            handlers: ActionHandlerRegistry::new(),
        }
    }
    
    /// Process an enforcement - execute or reverse based on its current state
    pub async fn process_enforcement(&self, 
                                    http: &Http, 
                                    enforcement_id: &str) -> Result<(), EnforcementError> {
        if let Some(mut record) = self.store.enforcements.get_mut(enforcement_id) {
            match record.state {
                EnforcementState::Pending => {
                    if record.execute_at <= Utc::now() {
                        self.execute_enforcement(http, &mut record).await?;
                    }
                },
                EnforcementState::Active => {
                    if let Some(reverse_at) = record.reverse_at {
                        if reverse_at <= Utc::now() {
                            self.reverse_enforcement(http, &mut record).await?;
                        }
                    }
                },
                _ => {}
            }
            Ok(())
        } else {
            Err(EnforcementError::NotFound(enforcement_id.to_string()))
        }
    }
    
    /// Execute a pending enforcement
    async fn execute_enforcement(&self, 
                               http: &Http, 
                               record: &mut dashmap::mapref::one::RefMut<'_, String, EnforcementRecord>) 
                               -> Result<(), EnforcementError> {
        let guild_id = GuildId::new(record.guild_id);
        let user_id = UserId::new(record.user_id);
        
        // Get the action type
        let action_type = record.action.get_type();
        
        // Get the handler
        if let Some(handler) = self.handlers.get_handler(action_type) {
            // Extract params based on action type
            let params = record.action.get_params();
            
            // Execute the action
            handler.execute(http, guild_id, user_id, params).await?;
            
            // Update the record state
            record.execute()?;
        }
        
        Ok(())
    }
    
    /// Reverse an active enforcement
    async fn reverse_enforcement(&self, 
                               http: &Http, 
                               record: &mut dashmap::mapref::one::RefMut<'_, String, EnforcementRecord>) 
                               -> Result<(), EnforcementError> {
        let guild_id = GuildId::new(record.guild_id);
        let user_id = UserId::new(record.user_id);
        
        // Get the action type
        let action_type = record.action.get_type();
        
        // Get the handler
        if let Some(handler) = self.handlers.get_handler(action_type) {
            // Extract params based on action type
            let params = record.action.get_params();
            
            // Reverse the action
            handler.reverse(http, guild_id, user_id, params).await?;
            
            // Update the record state
            record.reverse()?;
        }
        
        Ok(())
    }
    
    // Additional methods for creating and canceling enforcements
}
```

### 7. Migration Path

To migrate to the new system while maintaining backward compatibility:

1. Create new structs and types alongside existing ones
2. Implement conversion functions between old and new representations
3. Update core enforcement functionality to use new types internally
4. Gradually replace usage of old types with new ones
5. Maintain compatibility by converting between old and new representations in interface methods

Example wrapper for backward compatibility:

```rust
impl Data {
    // Provide backwards-compatible methods that use the new implementation
    
    pub fn get_pending_enforcements(&self) -> Vec<PendingEnforcement> {
        self.enforcement_service.store
            .enforcements
            .iter()
            .filter(|entry| entry.value().state == EnforcementState::Pending)
            .map(|entry| convert_to_legacy_format(entry.value()))
            .collect()
    }
    
    pub fn get_active_enforcements(&self) -> Vec<PendingEnforcement> {
        self.enforcement_service.store
            .enforcements
            .iter()
            .filter(|entry| entry.value().state == EnforcementState::Active)
            .map(|entry| convert_to_legacy_format(entry.value()))
            .collect()
    }
    
    // Other compatibility methods
}
```

## Implementation Timeline

1. **Phase 1: Define new types**
   - Create new action enum structure
   - Define error types
   - Implement state machine
   - Unit test new types

2. **Phase 2: Implement core services**
   - Create enforcement store
   - Implement action handlers
   - Create enforcement service
   - Unit test services

3. **Phase 3: Migrate with backward compatibility**
   - Create conversion functions
   - Update Data struct to use new services internally
   - Implement compatibility methods
   - End-to-end test to ensure existing functionality works

4. **Phase 4: Refactor commands**
   - Update commands to use new service
   - Remove duplicated code
   - Adjust logging and responses
   - Update error handling

5. **Phase 5: Cleanup and removal of legacy code**
   - Remove commented out code
   - Remove deprecated boolean flags
   - Consolidate enforcement maps
   - Final cleanup and documentation

## Benefits of Refactoring

This refactoring will:

1. **Reduce complexity**: Simpler state management with a proper state machine
2. **Eliminate duplication**: Common logic in shared functions/traits
3. **Improve type safety**: Better representation of actions and parameters
4. **Enhance error handling**: Specific error types with better context
5. **Simplify extension**: Adding new action types will be easier
6. **Increase testability**: More isolated components that can be unit tested
7. **Improve maintainability**: Clearer code organization and responsibility