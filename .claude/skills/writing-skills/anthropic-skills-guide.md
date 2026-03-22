# The Complete Guide to Building Skills for Claude

> Summarized from [Anthropic's official PDF guide](https://resources.anthropic.com/hubfs/The-Complete-Guide-to-Building-Skill-for-Claude.pdf) (verified 2026-03-22). This covers planning, testing, distribution, and the skill-creator tool. For authoring best practices, see anthropic-best-practices.md.

## Planning and Design

### Start with use cases

Before writing any code, identify 2-3 concrete use cases your skill should enable.

Good use case definition:
```
Use Case: Project Sprint Planning
Trigger: User says "help me plan this sprint" or "create sprint tasks"
Steps:
1. Fetch current project status from Linear (via MCP)
2. Analyze team velocity and capacity
3. Suggest task prioritization
4. Create tasks in Linear with proper labels and estimates
Result: Fully planned sprint with tasks created
```

### Common skill use case categories

1. **Document & Asset Creation** — Consistent, high-quality output (presentations, apps, designs, code)
2. **Workflow Automation** — Multi-step processes with consistent methodology, including coordination across multiple MCP servers
3. **MCP Enhancement** — Workflow guidance to enhance tool access an MCP server provides

### Define success criteria

**Quantitative:**
- Skill triggers on 90% of relevant queries (run 10-20 test queries)
- Completes workflow in X tool calls (compare with/without skill)
- 0 failed API calls per workflow

**Qualitative:**
- Users don't need to prompt Claude about next steps
- Workflows complete without user correction
- Consistent results across sessions

## The skill-creator

The `skill-creator` skill helps you build, test, and iterate on skills. Available via the Anthropic example-skills plugin (`/plugin install example-skills@anthropic-agent-skills`).

**Usage:** "Use the skill-creator skill to help me build a skill for [your use case]"

**What it does:**
- Generates skills from natural language descriptions
- Produces properly formatted SKILL.md with frontmatter
- Suggests trigger phrases and structure
- Flags common issues (vague descriptions, missing triggers, structural problems)
- Identifies potential over/under-triggering risks
- Suggests and runs test cases with parallel subagent execution (with-skill vs. baseline)
- Grades test outputs against assertions (`agents/grader.md`)
- Aggregates quantitative benchmarks — pass rate, timing, token usage (`scripts/aggregate_benchmark.py`)
- Provides an eval viewer for structured human feedback (`eval-viewer/generate_review.py`)
- Optimizes skill descriptions for triggering accuracy (`scripts/run_loop.py`)
- Packages skills for distribution (`scripts/package_skill.py`)

**Iterative improvement:** After running test cases, the eval viewer lets you review outputs and leave feedback. skill-creator reads your feedback and improves the skill, then reruns tests in a new iteration. Repeat until satisfied.

## Testing and Iteration

Three testing methods, choose based on your needs:
- **Manual testing in Claude.ai** — Fast iteration, no setup required
- **Scripted testing in Claude Code** — Automate test cases for repeatable validation
- **Programmatic testing via skills API** — Build evaluation suites against defined test sets

### Three testing areas

**1. Triggering tests** — Does the skill load at the right times?
- Should trigger on obvious tasks
- Should trigger on paraphrased requests
- Should NOT trigger on unrelated topics

**2. Functional tests** — Does the skill produce correct outputs?
- Valid outputs generated
- API calls succeed
- Error cases handled
- Edge cases covered

**3. Performance comparison** — Does the skill improve results vs. baseline?

Compare with and without skill: message count, token consumption, API failures, user corrections needed.

### Iteration based on feedback

**Undertriggering signals:** Skill doesn't load when it should, users manually enabling it, support questions about when to use it. **Solution:** Add more detail and keywords to the description.

**Overtriggering signals:** Skill loads for irrelevant queries, users disabling it, confusion about purpose. **Solution:** Add negative triggers, be more specific, clarify scope.

**Execution issues:** Inconsistent results, API call failures, user corrections needed. **Solution:** Improve instructions, add error handling.

**Instructions not followed:** Keep instructions concise, put critical instructions at the top with `## Important` or `## Critical` headers, be specific and actionable rather than vague.

## Distribution

### Where skills live

| Scope | How to install |
|-------|---------------|
| Personal | Place in `~/.claude/skills/` (Claude Code) or upload via Settings > Skills (Claude.ai) |
| Project | Commit `.claude/skills/` to version control |
| Organization | Admins deploy workspace-wide (shipped December 2025) |
| Plugin | Create `skills/` directory in your Claude Code plugin |
| API | Upload via `/v1/skills` endpoint, use `container.skills` parameter |

### Agent Skills open standard

Skills follow the [Agent Skills](https://agentskills.io) open standard. The same skill should work across Claude and other AI platforms. Use the `compatibility` frontmatter field to note environment requirements.

## Skill Patterns

### Pattern 1: Sequential workflow orchestration
**Use when:** Multi-step processes in specific order. Key techniques: explicit step ordering, dependencies between steps, validation at each stage.

### Pattern 2: Multi-MCP coordination
**Use when:** Workflows span multiple services. Key techniques: clear phase separation, data passing between MCPs, validation before next phase.

### Pattern 3: Iterative refinement
**Use when:** Output quality improves with iteration. Key techniques: explicit quality criteria, validation scripts, know when to stop iterating.

### Pattern 4: Context-aware tool selection
**Use when:** Same outcome, different tools depending on context. Key techniques: clear decision criteria, fallback options, transparency about choices.

### Pattern 5: Domain-specific intelligence
**Use when:** Skill adds specialized knowledge beyond tool access. Key techniques: domain expertise embedded in logic, compliance before action, comprehensive documentation.

## Troubleshooting Quick Reference

| Problem | Common Cause | Fix |
|---------|-------------|-----|
| Skill won't upload | SKILL.md not found or wrong case | Rename to exactly `SKILL.md` (case-sensitive) |
| Invalid frontmatter | Missing `---` delimiters or unclosed quotes | Check YAML syntax |
| Invalid skill name | Spaces or capitals in name | Use kebab-case only |
| Skill doesn't trigger | Description too generic or missing triggers | Add trigger phrases users would say |
| Skill triggers too often | Description too broad | Add negative triggers, be more specific |
| Instructions not followed | Instructions too verbose or buried | Keep concise, put critical info at top |
| Large context issues | Skill content too large | Move docs to `references/`, keep SKILL.md under 5,000 words |
