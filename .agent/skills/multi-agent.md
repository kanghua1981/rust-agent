# Multi-Agent Orchestration Skill

This skill allows the agent to decompose complex tasks and delegate them to specialized sub-agents running in server mode. This improves efficiency, maintains directory isolation, and allows for parallel or specialized processing.

## When to use Sub-Agents
- **Monorepos**: When changes are needed in a specific sub-directory (e.g., `frontend/`, `backend/`, `docs/`).
- **Specialization**: When a task requires a specific model or persona (e.g., a "Security Auditor" or "CSS Expert").
- **Parallel Tasks**: When independent components can be worked on simultaneously.
- **Complexity Management**: When the main context window is getting full and you need a fresh start for a specific module.

## How to use `call_sub_agent`
1.  **Identify the Sub-task**: Clearly define what needs to be done.
2.  **Determine the Server URL**: Usually `ws://localhost:9527` for a local server, or a remote IP/Port.
3.  **Specify `target_dir`**: Provide the relative path to the directory the sub-agent should focus on.
4.  **Formulate the Prompt**: Be specific. Mention any local constraints.
5.  **Set `auto_approve`**:
    - Use `false` (default) for dangerous tasks (deletes, major refactors).
    - Use `true` for trusted, non-destructive, or highly specific tasks where you've already approved the high-level plan.

### Example: Config in models.toml
```toml
[sub_agents.frontend]
port = 9527
role = "coder"

[sub_agents.backend]
port = 9528
role = "coder"
```

### Example: delegating frontend work
```json
{
  "name": "call_sub_agent",
  "arguments": {
    "prompt": "Update the UserProfile component to include a new 'Avatar' field using the provided SVG assets in /assets.",
    "server_url": "ws://localhost:9527",
    "target_dir": "src/frontend",
    "auto_approve": false
  }
}
```

## Best Practices for Orchestration
- **Context Passing**: If the sub-agent needs info from outside its `target_dir`, include it in the `prompt`.
- **Incremental Merging**: Review the final output of the sub-agent before committing changes if they were performed in a shared repository.
- **Transparency**: Always check the `[Sub-Agent]` logs in the UI to ensure the delegation is proceeding as expected.
- **Recursive Safety**: Avoid calling another sub-agent from within a sub-agent unless the hierarchy is clear and loops are impossible.
