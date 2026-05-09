# Commit Workflow

Use a subagent to handle the commit:
1. Spawn a general-purpose agent to stage changes and create a commit
2. The agent should run `git add -A` and `git commit` with a descriptive message summarizing the changes
