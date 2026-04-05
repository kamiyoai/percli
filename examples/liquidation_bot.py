#!/usr/bin/env python3
"""
Liquidation bot agent — monitors accounts and liquidates any that fall
below the maintenance margin while holding a position.
"""
import sys
import json

for line in sys.stdin:
    msg = json.loads(line)
    if msg["type"] == "done":
        break
    if msg["type"] != "tick":
        continue

    actions = []
    for acct in msg["snapshot"]["accounts"]:
        if not acct["above_maintenance_margin"] and acct["effective_position_q"] != 0:
            actions.append({"op": "liquidate", "account": acct["name"]})

    print(json.dumps({"actions": actions}), flush=True)
