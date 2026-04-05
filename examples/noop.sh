#!/bin/bash
# Noop agent — responds with empty actions to every tick.
while IFS= read -r line; do
  type=$(echo "$line" | sed -n 's/.*"type":"\([^"]*\)".*/\1/p')
  [ "$type" = "done" ] && exit 0
  echo '{"actions":[]}'
done
