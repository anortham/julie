#!/bin/sh
# Fails when new product code adds a coordination word (design section 4).
# Usage: sh scripts/complexity-words.sh [<base>]   (default base: main)
# Allowlist one line by adding "complexity-exception: <reason>" to it.
# ".lock(" method calls on in-process mutexes are not coordination and are skipped.
base="${1:-main}"
words='lock|lease|fence|generation|epoch|cursor|claim|pin|coordinator|broker|journal|repair|continuation|handoff'

hits=$(git diff --unified=0 "$base...HEAD" -- '*.rs' ':!**/tests/**' ':!src/tests/**' |
  awk '
    /^\+\+\+ / { file = substr($2, 3); next }
    /^@@ /     { split($3, pos, /[+,]/); line = pos[2]; next }
    /^\+/      { print file ":" line ": " substr($0, 2); line++ }
  ' |
  grep -v 'complexity-exception:' |
  sed 's/\.lock(/.LOCKMETHOD(/g' |
  grep -Ewi "$words" |
  sed 's/\.LOCKMETHOD(/.lock(/g')

if [ -n "$hits" ]; then
  echo "$hits"
  echo "complexity words found in code added since $base"
  exit 1
fi
echo "no complexity words added since $base"
