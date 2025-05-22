# Enforcement System Refactoring

This document provides an overview of the refactored enforcement system implemented in the Dastardly Daemon bot.

## Overview

The enforcement system has been refactored to improve:

1. **Code Organization**: Clear separation of concerns with dedicated components
2. **Type Safety**: Better representation of actions and parameters
3. **State Management**: Proper state machine for enforcement lifecycle
4. **Error Handling**: Specific error types with better context
5. **Testability**: More isolated components for unit testing
6. **Maintainability**: Reduced code duplication and complexity

The new system is implemented in the `enforcement_new` module and integrates with the existing system through the `data_ext` module.

## Architecture

### Key Components

1. **EnforcementAction**: Enum representing different types of enforcement actions
2. **ActionParams**: Common parameters for enforcement actions
3. **EnforcementRecord**: Record of an enforcement action and its lifecycle
4. **EnforcementState**: State machine for enforcement lifecycle
5. **EnforcementStore**: Central store for enforcement records
6. **ActionHandler**: Trait for handling execution and reversal of actions
7. **EnforcementService**: Orchestrates enforcement operations

### Data Flow

```
Command -> EnforcementService -> ActionHandler -> Discord API
                |
                v
         EnforcementStore
```

1. Commands create an enforcement record through the EnforcementService
2. Records are stored in the EnforcementStore with state=Pending
3. The enforcement task periodically checks for records due for execution
4. The appropriate ActionHandler executes the action on the Discord API
5. Record state transitions to Active (if needs reversal) or Completed
6. For active records, when reversal time is reached, the action is reversed

## Usage

### Creating an Enforcement

```rust
// Create an enforcement action
let action = EnforcementAction::mute(300); // 300 seconds

// Create an enforcement record
let record = data.create_enforcement(
    warning_id,
    user_id,
    guild_id,
    action,
);

// Optionally notify for immediate execution
data.notify_enforcement_about_user(user_id, guild_id).await?;
```

### Cancelling Enforcements

```rust
// Cancel a specific enforcement
data.process_enforcement(http, &enforcement_id).await?;

// Cancel all enforcements for a user
data.cancel_user_enforcements(http, user_id, guild_id).await?;
```

## Backward Compatibility

The new system maintains backward compatibility with the existing system through:

1. **Data Extension Layer**: The `data_ext` module provides compatibility methods
2. **Dual Storage**: Records are stored in both new and old systems during transition
3. **Import/Export**: Data can be synchronized between old and new systems

## Error Handling

The new system uses a dedicated `EnforcementError` type which provides:

1. Better context for errors
2. Specific error variants for different scenarios
3. Consistent error propagation
4. Integration with thiserror for nice Display impls

Example:

```rust
pub enum EnforcementError {
    #[error("Invalid state transition")]
    InvalidStateTransition,
    
    #[error("Enforcement not found: {0}")]
    NotFound(String),
    
    #[error("Discord API error: {0}")]
    DiscordApi(#[from] poise::serenity_prelude::Error),
    
    // ...more error types
}
```

## Migration Path

The refactoring is implemented alongside the existing system to allow for gradual migration:

### Phase 1: Dual Operations
- Both systems operate in parallel
- New commands use the new system through the compatibility layer
- Old commands continue to use the old system
- Records are synchronized between systems

### Phase 2: Full Migration
- Switch all commands to use the new system
- Remove the old system code
- Maintain only one set of data structures

## Benefits

This refactoring addresses several issues in the original implementation:

1. **Reduced Complexity**: The new state machine simplifies the lifecycle management
2. **Eliminated Duplication**: Common functions are now shared
3. **Improved Type Safety**: Better parameter representation
4. **Enhanced Error Handling**: More specific error types
5. **Easier Extensions**: Adding new action types is simpler
6. **Better Testability**: Isolated components are easier to test
7. **Improved Maintainability**: Clearer organization and responsibility

## Testing

The new system includes comprehensive unit tests for:

1. Action management
2. State transitions
3. Store operations
4. Error handling
5. Parameter validation

## Future Improvements

Potential future enhancements:

1. **Metrics Collection**: Track success/failure rates
2. **Admin Dashboard**: Visual monitoring of enforcement actions
3. **Rate Limiting**: Prevent excessive enforcements
4. **Bulk Operations**: Apply enforcements to multiple users
5. **Enhanced Logging**: More detailed logs for auditing