#!/usr/bin/env bash
# Example bundled script for the {{TOPIC_NAME}} skill.
#
# Files in skill/scripts/ ship into the rendered skill's scripts/ folder
# (executable bit preserved). Reference this file from a `skill_script` block in
# data/skill/artifacts.wcl. Replace this with a real helper, or delete both.
set -euo pipefail

echo "Hello from the {{TOPIC_NAME}} skill. Replace me with something useful."
