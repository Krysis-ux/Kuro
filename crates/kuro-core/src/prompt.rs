//! Telling the model where it is and what it can do.
//!
//! Without this, a model has no idea it is running inside Kuro, that a web search
//! tool exists, or that the paragraph above its prompt is a set of live search
//! results. The observed failure modes are all the same mistake:
//!
//! * asked "do you have internet access", it answers from whatever its training
//!   data said about its own deployment — which is never this one;
//! * asked to look something up, it produces plausible-looking URLs it invented,
//!   because nothing told it that inventing them is not allowed;
//! * handed real search results, it ignores them, because nothing told it they
//!   were real.
//!
//! The prompt is written for the *small* end of the model range. A 0.5B model is
//! the one that needs this most and is also the one that drowns in a long brief,
//! so every line here has to earn its place. Capability statements are phrased as
//! present-tense facts ("You can search the web right now") rather than
//! conditionals, because small models handle facts far better than they handle
//! "if the user has enabled X then you may Y".

use crate::skills::Skill;

/// What the model needs to know about this turn.
#[derive(Debug, Clone)]
pub struct PromptContext<'a> {
    /// The model's own id, so it can answer "what are you running as".
    pub model_id: &'a str,
    /// True when the request goes to a provider rather than a local engine.
    pub is_remote: bool,
    /// Whether the web tool group is on for this message.
    pub web_enabled: bool,
    /// Whether Kuro already ran a search and put the results in the context.
    pub search_ran: bool,
    pub memory_enabled: bool,
    /// How many memories exist, so "you remember nothing yet" can be said plainly.
    pub memory_count: i64,
    /// Tool names offered this turn, exactly as the model will see them.
    pub tool_names: &'a [String],
    /// Skills the user switched on.
    pub skills: &'a [&'a Skill],
}

/// Build the system prompt for one turn.
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

    out
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

/// The live state of every switch, stated as fact.
fn capabilities(context: &PromptContext<'_>) -> String {
    let mut out = String::from("Right now, in this conversation:\n");

    if context.search_ran {
        out.push_str(
            "- You have web access. The user's question was already searched for, and the \
             results appear in this context as \"Web results\". They are real and current. \
             Use them.\n",
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

    out.push_str(
        "- You can see the whole conversation above, including everything the user has \
         said in it. Answer questions about it directly.\n\n",
    );

    out
}

fn tools(context: &PromptContext<'_>) -> String {
    format!(
        "Tools you may call this turn: {}.\n\
         Call a tool by name when it is the right way to answer. Do not describe calling \
         a tool instead of calling it, and do not claim you have called one when you have not.\n\n",
        context.tool_names.join(", ")
    )
}

/// The rules that stop the specific failures seen in real transcripts.
fn honesty(context: &PromptContext<'_>) -> String {
    let mut out = String::from("Rules:\n");

    // Invented URLs were the worst observed failure: a question about a net worth
    // produced five fabricated links, each of which looked entirely plausible.
    out.push_str(
        "- Never invent a URL, a citation, a price, a statistic or a date. If you did not \
         get it from a tool result or from the user, you do not have it.\n",
    );

    if context.search_ran || context.web_enabled {
        out.push_str(
            "- Only cite URLs that appear verbatim in a tool result. Quote them exactly.\n",
        );
    }

    out.push_str(
        "- Say \"I don't know\" when you do not know. A wrong confident answer is worse \
         than an admission.\n\
         - Do not describe your own limitations from memory. What you can do this turn is \
         listed above, and that list is authoritative.\n\
         - Answer the question that was asked. Skip preamble and restatement.\n\n",
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
            skills: &[],
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
        // Asked "what was my first message", a small model otherwise refuses.
        assert!(build(&context()).contains("see the whole conversation"));
    }

    #[test]
    fn fabrication_is_forbidden_explicitly() {
        let prompt = build(&context());
        assert!(prompt.contains("Never invent a URL"));
        assert!(prompt.contains("I don't know"));
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
        let prompt = build(&PromptContext {
            web_enabled: true,
            search_ran: true,
            memory_enabled: true,
            memory_count: 3,
            tool_names: &names,
            ..context()
        });

        let words = prompt.split_whitespace().count();
        assert!(
            words < 400,
            "a 0.5B model has little context to spare; got {words} words"
        );
    }
}
