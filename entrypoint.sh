#!/bin/sh

# Read the secret from the file and export it as an environment variable
if [ -f /run/secrets/DISCORD_TOKEN ]; then
  export DISCORD_TOKEN=$(cat /run/secrets/DISCORD_TOKEN)
fi

# Execute the main container command
exec "$@"
