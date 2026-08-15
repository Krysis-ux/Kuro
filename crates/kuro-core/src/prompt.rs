
use crate::skills::Skill;
use crate::workspace::WorkspaceMode;

#[derive(Debug, Clone)]
pub struct PromptContext<'a> {
    pub model_id: &'a str,
    pub is_remote: bool,
    pub web_enabled: bool,
    pub search_ran: bool,
    pub memory_enabled: bool,
    pub memory_count: i64,
    pub tool_names: &'a [String],
    pub mcp_servers: &'a [String],
    pub workspace: Option<WorkspaceBrief<'a>>,
    pub projects_readable: bool,
    pub skills: &'a [&'a Skill],
    pub project: Option<ProjectBrief<'a>>,
}

#[derive(Debug, Clone, Copy)]
pub struct WorkspaceBrief<'a> {
    pub name: &'a str,
    pub root: &'a str,
    pub mode: WorkspaceMode,
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectBrief<'a> {
    pub name: &'a str,
    pub instructions: &'a str,
}

pub fn build(context: &PromptContext<'_>) -> String {
    let mut out = String::with_capacity(1200);

    out.push_str(&identity(context));
    out.push_str(&capabilities(context));

    if !context.tool_names.is_empty() {
        out.push_str(&tools(context));
    }

    out.push_str(&honesty(context));

    if !context.skills.is_empty() {
        out.push_str(&skills(context));
    }

    if let Some(project) = &context.project {
        out.push_str(&project_section(project));
    }

    out
}

fn project_section(project: &ProjectBrief<'_>) -> String {
    let instructions = project.instructions.trim();
    if instructions.is_empty() {
        return format!(
            "This conversation is in the project \"{}\".\n\n",
            project.name
        );
    }

    format!(
        "This conversation is in the project \"{}\". \
         Its standing instructions, which take precedence over the general guidance above:\n\n{}\n\n",
        project.name, instructions
    )
}

fn identity(context: &PromptContext<'_>) -> String {
    let where_it_runs = if context.is_remote {
        "You are reached through a provider API that the user pays for themselves."
    } else {
        "You are running locally on the user's own computer. Nothing they say to you \
         leaves this machine unless a tool below explicitly sends it."
    };

    format!(
        "You are an assistant running inside Kuro, a local model server. \
         You are the model `{}`. {}\n\n",
        context.model_id, where_it_runs
    )
}

fn capabilities(context: &PromptContext<'_>) -> String {
    let mut out = String::from("Right now, in this conversation:\n");

    if context.search_ran {
        out.push_str(
            "- You have web access. The user's question was already searched for, and the \
             results appear in this context as \"Web results\". They are real and current. \
             Read them and write the answer yourself, in your own words. Do not reply with \
             only a list of links — the interface already shows the sources under your reply.\n",
        );
    } else if context.web_enabled {
        out.push_str(
            "- You have web access, through the `web_search` and `fetch_url` tools. \
             Call `web_search` whenever the answer depends on current information, or on \
             anything you are not certain of.\n",
        );
    } else {
        out.push_str(
            "- You have NO web access this turn. The user has web search switched off. \
             You cannot browse, open links, or check anything online. If they ask you to \
             look something up, say plainly that web access is off and that the Web switch \
             below the message box turns it on.\n",
        );
    }

    if context.memory_enabled {
        if context.memory_count > 0 {
            out.push_str(
                "- You have memory. Facts saved earlier appear above under \"Things you have \
                 been asked to remember\". Use `remember` to save a new durable fact, and \
                 `recall` to search for one.\n",
            );
        } else {
            out.push_str(
                "- You have memory, but nothing has been saved yet. Use `remember` when the \
                 user tells you something worth keeping for next time.\n",
            );
        }
    } else {
        out.push_str("- Memory is off this turn. You cannot save or look up saved facts.\n");
    }

    out.push_str(&files_line(context));

    if !context.mcp_servers.is_empty() {
        out.push_str(&format!(
            "- Connected MCP tool servers: {}. These are the servers the user has connected to \
             Kuro. If they ask what is connected, this list is the answer.\n",
            context.mcp_servers.join(", ")
        ));
    }

    out.push_str(
        "- You can see the whole conversation above, including everything the user has \
         said in it. Answer questions about it directly.\n\n",
    );

    out
}

fn files_line(context: &PromptContext<'_>) -> String {
    let Some(workspace) = &context.workspace else {
        let projects = if context.projects_readable {
            " You CAN read the coding workspaces they opened, with `list_projects`, \
             `read_project_file` and `search_projects`. That is read-only."
        } else {
            ""
        };

        return format!(
            "- You CANNOT change any file on this computer, and no setting in this \
             conversation changes that.{projects} If they ask you to edit something, say \
             that editing happens in a coding workspace on the Code page.\n"
        );
    };

    let root = workspace.root;
    let name = workspace.name;
    let shell = crate::workspace::exec::shell_description();
    let platform = crate::workspace::exec::platform_note();

    match workspace.mode {
        WorkspaceMode::Ask => format!(
            "- You are in the `{name}` workspace ({root}), but it is in Ask mode, so you \
             have NO file tools this turn. Discuss the code from what the user shows you. \
             If you need to see the project, say that Plan mode would let you read it.\n"
        ),
        WorkspaceMode::Plan => format!(
            "- You are in the `{name}` workspace, and can READ it: {root}. You may list, \
             read and search files there, and nothing outside it. You CANNOT change \
             anything this turn and you CANNOT run commands — propose the edit and say \
             which file and line it goes in. Read a file before describing it; never guess \
             at its contents.\n"
        ),
        WorkspaceMode::Agent | WorkspaceMode::Bypass => {
            let commands = if workspace.mode == WorkspaceMode::Bypass {
                "any command, with no allowlist"
            } else {
                "build, test and package commands; anything else is refused, and says so"
            };

            format!(
                "- You are in the `{name}` workspace and can READ AND CHANGE it: {root}. \
                 Anything outside is refused. Read a file before editing it, and prefer \
                 `edit_file` over `write_file`, which replaces the whole file. Every change \
                 can be undone, so say plainly what you changed.\n\
                 - `run_command` runs {commands} there, through {shell}. Check your work by \
                 running it — do not describe a command instead of calling it.\n\
                 - {platform}\n\
                 - `start_server` runs a dev server in the background and the user sees the \
                 page in a preview panel. `check_server` reads its output, `stop_server` \
                 ends it.\n"
            )
        }
    }
}

fn tools(context: &PromptContext<'_>) -> String {
    format!(
        "Tools you may call this turn: {}.\n\
         Call a tool by name when it is the right way to answer. Do not describe calling \
         a tool instead of calling it, and do not claim you have called one when you have not.\n\n",
        context.tool_names.join(", ")
    )
}

fn honesty(context: &PromptContext<'_>) -> String {
    let mut out = String::from("Rules:\n");

    out.push_str(
        "- Reply like a person would. A greeting gets a greeting, small talk gets small talk, \
         and a question about you gets an answer from the list above. Never answer any of \
         these with \"I don't know\".\n\
         - Answer the question that was asked. Skip preamble and restatement.\n",
    );

    out.push_str(
        "- Never invent a URL, a citation, a price, a statistic or a date. If you did not \
         get it from a tool result or from the user, you do not have it.\n",
    );

    if context.search_ran {
        out.push_str(
            "- Answer from the web results above. Pull the facts out of them and put them \
             together into a direct answer. If they do not cover the question, say which part \
             they did not answer — do not discard the rest of what they do say.\n\
             - Only cite URLs that appear verbatim in those results. Quote them exactly.\n",
        );
    } else if context.web_enabled {
        out.push_str(
            "- Only cite URLs that appear verbatim in a tool result. Quote them exactly.\n",
        );
    }

    out.push_str(
        "- Do not describe your own limitations from memory. What you can do this turn is \
         listed above, and that list is authoritative.\n\
         - If you are asked for a specific fact you do not have, say which fact is missing \
         and how it could be found. \"I don't know\" on its own is never a complete reply.\n\n",
    );

    out
}

fn skills(context: &PromptContext<'_>) -> String {
    let mut out = String::from("Active skills:\n\n");

    for skill in context.skills {
        out.push_str(&format!("## {}\n{}\n\n", skill.name, skill.instructions.trim()));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills;

    fn context() -> PromptContext<'static> {
        PromptContext {
            model_id: "qwen3-4b:q4_k_m",
            is_remote: false,
            web_enabled: false,
            search_ran: false,
            memory_enabled: false,
            memory_count: 0,
            tool_names: &[],
            mcp_servers: &[],
            workspace: None,
            projects_readable: false,
            skills: &[],
            project: None,
        }
    }

    #[test]
    fn the_model_is_told_where_it_is_and_what_it_is() {
        let prompt = build(&context());

        assert!(prompt.contains("Kuro"), "the model must know the application it is in");
        assert!(prompt.contains("qwen3-4b:q4_k_m"), "and which model it is");
        assert!(prompt.contains("locally"), "and that it runs on this machine");
    }

    #[test]
    fn a_provider_model_is_not_told_it_runs_locally() {
        let prompt = build(&PromptContext {
            is_remote: true,
            ..context()
        });

        assert!(prompt.contains("provider API"));
        assert!(
            !prompt.contains("running locally"),
            "claiming a remote model is local would be a lie about where data goes"
        );
    }

    #[test]
    fn with_web_off_the_model_is_told_so_in_the_strongest_terms() {
        let prompt = build(&context());

        assert!(prompt.contains("NO web access"));
        assert!(prompt.contains("Web switch"), "it should say how to turn it on");
        assert!(!prompt.contains("You have web access"));
    }

    #[test]
    fn with_web_on_the_model_is_told_which_tools_to_call() {
        let prompt = build(&PromptContext {
            web_enabled: true,
            tool_names: &[],
            ..context()
        });

        assert!(prompt.contains("You have web access"));
        assert!(prompt.contains("web_search"));
        assert!(!prompt.contains("NO web access"));
    }

    #[test]
    fn when_a_search_already_ran_the_results_are_pointed_at() {
        let prompt = build(&PromptContext {
            web_enabled: true,
            search_ran: true,
            ..context()
        });

        assert!(prompt.contains("already searched"));
        assert!(prompt.contains("Web results"), "it must know what to look for in context");
        assert!(prompt.contains("real and current"));
    }

    #[test]
    fn memory_state_is_stated_either_way() {
        let empty = build(&PromptContext {
            memory_enabled: true,
            memory_count: 0,
            ..context()
        });
        assert!(empty.contains("nothing has been saved yet"));

        let populated = build(&PromptContext {
            memory_enabled: true,
            memory_count: 4,
            ..context()
        });
        assert!(populated.contains("asked to remember"));

        let off = build(&context());
        assert!(off.contains("Memory is off"));
    }

    #[test]
    fn the_model_is_told_it_can_see_the_conversation() {
        assert!(build(&context()).contains("see the whole conversation"));
    }

    #[test]
    fn fabrication_is_forbidden_explicitly() {
        let prompt = build(&context());
        assert!(prompt.contains("Never invent a URL"));
    }

    #[test]
    fn replying_normally_is_the_first_rule_and_not_knowing_is_the_last() {
        let prompt = build(&context());

        let rules = prompt.find("Rules:").expect("rules section");
        let reply_normally = prompt.find("Reply like a person").expect("present");
        let not_knowing = prompt.find("say which fact is missing").expect("present");

        assert!(reply_normally > rules);
        assert!(
            reply_normally < not_knowing,
            "answering normally must be read before the caveat about not knowing"
        );
    }

    #[test]
    fn a_greeting_is_explicitly_excluded_from_i_dont_know() {
        let prompt = build(&context());

        assert!(prompt.contains("A greeting gets a greeting"));
        assert!(
            prompt.contains("Never answer any of these with \"I don't know\""),
            "the failure was general enough to need naming outright"
        );
        assert!(
            prompt.contains("\"I don't know\" on its own is never a complete reply"),
            "a bare admission is what the transcripts actually produced"
        );
    }

    #[test]
    fn search_results_must_be_synthesised_rather_than_listed() {
        let prompt = build(&PromptContext {
            web_enabled: true,
            search_ran: true,
            ..context()
        });

        assert!(prompt.contains("write the answer yourself"));
        assert!(prompt.contains("only a list of links"));
        assert!(prompt.contains("Pull the facts out of them"));
    }

    #[test]
    fn connected_mcp_servers_are_named_so_the_question_can_be_answered() {
        let servers = vec!["Filesystem".to_string(), "Context7".to_string()];
        let prompt = build(&PromptContext {
            mcp_servers: &servers,
            ..context()
        });

        assert!(prompt.contains("Connected MCP tool servers"));
        assert!(prompt.contains("Filesystem"));
        assert!(prompt.contains("Context7"));
        assert!(prompt.contains("this list is the answer"));
    }

    #[test]
    fn no_mcp_line_appears_when_nothing_is_connected() {
        assert!(!build(&context()).contains("Connected MCP tool servers"));
    }

    #[test]
    fn a_chat_is_told_plainly_that_it_cannot_change_any_file() {
        let prompt = build(&context());

        assert!(prompt.contains("CANNOT change any file"));
        assert!(
            prompt.contains("coding workspace"),
            "it should say where editing does live"
        );
        assert!(
            !prompt.contains("read_project_file"),
            "a chat without the projects switch should not be told about a tool it does not have"
        );
    }

    #[test]
    fn a_chat_that_can_read_projects_is_told_it_is_read_only() {
        let prompt = build(&PromptContext {
            projects_readable: true,
            ..context()
        });

        assert!(prompt.contains("read_project_file"));
        assert!(prompt.contains("read-only"));
        assert!(
            prompt.contains("CANNOT change any file"),
            "being able to read must not read as being able to write"
        );
    }

    #[test]
    fn a_workspace_turn_is_not_also_told_about_the_weaker_project_tools() {
        let prompt = build(&PromptContext {
            workspace: Some(WorkspaceBrief {
                name: "App",
                root: "/tmp/app",
                mode: WorkspaceMode::Plan,
            }),
            projects_readable: true,
            ..context()
        });

        assert!(!prompt.contains("read_project_file"));
    }

    #[test]
    fn a_writable_workspace_is_told_it_can_run_things_and_look_at_them() {
        let prompt = build(&PromptContext {
            workspace: Some(WorkspaceBrief {
                name: "App",
                root: "/tmp/app",
                mode: WorkspaceMode::Agent,
            }),
            ..context()
        });

        assert!(prompt.contains("run_command"));
        assert!(prompt.contains("start_server"));
        assert!(prompt.contains("preview panel"));
        assert!(
            prompt.contains("do not describe a command instead of calling it"),
            "the whole point is that it runs the command instead of narrating it"
        );
        assert!(
            prompt.contains("refused"),
            "Agent mode's allowlist should be stated, not discovered"
        );
    }

    #[test]
    fn bypass_is_told_the_allowlist_is_off_rather_than_told_nothing() {
        let prompt = build(&PromptContext {
            workspace: Some(WorkspaceBrief {
                name: "App",
                root: "/tmp/app",
                mode: WorkspaceMode::Bypass,
            }),
            ..context()
        });

        assert!(prompt.contains("no allowlist"));
    }

    #[test]
    fn the_workspace_root_is_named_and_the_mode_is_stated() {
        let brief = |mode| WorkspaceBrief { name: "Kuro", root: "/Users/me/Projects", mode };

        let planning = build(&PromptContext {
            workspace: Some(brief(WorkspaceMode::Plan)),
            ..context()
        });
        assert!(planning.contains("/Users/me/Projects"));
        assert!(planning.contains("CANNOT change"));

        let writable = build(&PromptContext {
            workspace: Some(brief(WorkspaceMode::Agent)),
            ..context()
        });
        assert!(writable.contains("READ AND CHANGE"));
        assert!(writable.contains("edit_file"));

        let asking = build(&PromptContext {
            workspace: Some(brief(WorkspaceMode::Ask)),
            ..context()
        });
        assert!(asking.contains("NO file tools"));
        assert!(
            writable.contains("Anything outside is refused"),
            "the boundary matters more than the capability"
        );
    }

    #[test]
    fn citation_discipline_appears_only_when_there_is_something_to_cite() {
        let with_web = build(&PromptContext {
            web_enabled: true,
            ..context()
        });
        assert!(with_web.contains("Only cite URLs"));

        assert!(
            !build(&context()).contains("Only cite URLs"),
            "a rule about citations is noise when no tool can produce one"
        );
    }

    #[test]
    fn the_model_is_told_not_to_describe_its_limits_from_memory() {
        assert!(build(&context()).contains("authoritative"));
    }

    #[test]
    fn offered_tools_are_named_verbatim() {
        let names = vec!["web_search".to_string(), "context7_query-docs".to_string()];
        let prompt = build(&PromptContext {
            tool_names: &names,
            ..context()
        });

        assert!(prompt.contains("context7_query-docs"), "the exact callable name matters");
        assert!(prompt.contains("Do not describe calling a tool"));
    }

    #[test]
    fn no_tool_section_appears_when_there_are_no_tools() {
        assert!(!build(&context()).contains("Tools you may call"));
    }

    #[test]
    fn active_skills_are_appended_with_their_instructions() {
        let rust = skills::find("rust").expect("rust skill");
        let prompt = build(&PromptContext {
            skills: &[rust],
            ..context()
        });

        assert!(prompt.contains("Active skills"));
        assert!(prompt.contains(&rust.name.to_string()));
        assert!(prompt.contains("Result"), "the rust skill's own guidance should be present");
    }

    #[test]
    fn the_prompt_stays_short_enough_for_a_small_model() {
        let names: Vec<String> = (0..6).map(|i| format!("tool_{i}")).collect();
        let servers = vec!["Documentation".to_string()];
        let prompt = build(&PromptContext {
            web_enabled: true,
            search_ran: true,
            memory_enabled: true,
            memory_count: 3,
            tool_names: &names,
            mcp_servers: &servers,
            workspace: Some(WorkspaceBrief {
                name: "Kuro",
                root: "/Users/me/Projects",
                mode: WorkspaceMode::Agent,
            }),
            ..context()
        });

        let words = prompt.split_whitespace().count();
        assert!(
            words < 560,
            "a 0.5B model has little context to spare; got {words} words"
        );
    }
}

#[cfg(test)]
mod project_tests {
    use super::*;

    fn base() -> PromptContext<'static> {
        PromptContext {
            model_id: "m",
            is_remote: false,
            web_enabled: false,
            search_ran: false,
            memory_enabled: false,
            memory_count: 0,
            tool_names: &[],
            mcp_servers: &[],
            workspace: None,
            projects_readable: false,
            skills: &[],
            project: None,
        }
    }

    #[test]
    fn project_instructions_are_included_and_named() {
        let prompt = build(&PromptContext {
            project: Some(ProjectBrief {
                name: "Kuro",
                instructions: "Assume Rust 2021. Never suggest adding a dependency.",
            }),
            ..base()
        });

        assert!(prompt.contains("project \"Kuro\""));
        assert!(prompt.contains("Never suggest adding a dependency"));
        assert!(prompt.contains("take precedence"));
    }

    #[test]
    fn a_project_with_no_instructions_is_still_named() {
        let prompt = build(&PromptContext {
            project: Some(ProjectBrief {
                name: "Scratch",
                instructions: "   ",
            }),
            ..base()
        });

        assert!(prompt.contains("project \"Scratch\""));
        assert!(
            !prompt.contains("take precedence"),
            "there is nothing to take precedence over"
        );
    }

    #[test]
    fn project_instructions_come_last() {
        let prompt = build(&PromptContext {
            project: Some(ProjectBrief {
                name: "Kuro",
                instructions: "PROJECT_MARKER",
            }),
            ..base()
        });

        let marker = prompt.find("PROJECT_MARKER").expect("present");
        let rules = prompt.find("Rules:").expect("present");
        assert!(marker > rules, "the user's own instructions should read last");
    }

    #[test]
    fn no_project_section_appears_without_a_project() {
        assert!(!build(&base()).contains("This conversation is in the project"));
    }
}
