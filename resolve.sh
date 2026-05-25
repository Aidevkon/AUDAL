#!/bin/bash
export GIT_EDITOR=true
while true; do
    status=$(git status 2>&1)
    if echo "$status" | grep -q "Unmerged paths"; then
        git checkout --ours .
        git checkout --theirs RESEARCH.md 2>/dev/null || true
        git add .
        git rebase --continue || git rebase --skip
    elif echo "$status" | grep -q "all conflicts fixed"; then
        git rebase --continue || git rebase --skip
    elif echo "$status" | grep -q "rebase in progress"; then
        git rebase --skip
    else
        echo "Rebase finished or unknown state"
        break
    fi
done
