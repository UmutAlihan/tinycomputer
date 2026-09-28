//! The scenario ladder itself: every lab task, easiest first.

use super::{Check, Reset, Scenario};

/// Every scenario, easiest first.
pub const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "textedit",
        app: "TextEdit",
        brief: include_str!("../../../scenarios/textedit/brief.md"),
        flow: include_str!("../../../scenarios/textedit/flow.json"),
        goal: "Start a new blank TextEdit document and type the supplied paragraph into it.",
        texts: &[
            "Desktop flows describe what to do, not how. Jev grounds every step on the live screen. This paragraph was written by a tinycomputer flow.",
        ],
        check: Check::TextEditContains("This paragraph was written by a tinycomputer flow."),
        reset: &[Reset::AppleScript(
            r#"tell application "TextEdit" to close every document saving no"#,
        )],
    },
    Scenario {
        name: "calculator",
        app: "Calculator",
        brief: include_str!("../../../scenarios/calculator/brief.md"),
        flow: include_str!("../../../scenarios/calculator/flow.json"),
        goal: "Calculate 128 multiplied by 37 by pressing the Calculator buttons until the display shows 4736.",
        texts: &[],
        check: Check::CalculatorShows("4736"),
        reset: &[Reset::Press("escape"), Reset::Press("escape")],
    },
    Scenario {
        name: "notes",
        app: "Notes",
        brief: include_str!("../../../scenarios/notes/brief.md"),
        flow: include_str!("../../../scenarios/notes/flow.json"),
        goal: "Create a new note titled tinycomputer lab note {run} with the supplied text.",
        texts: &[
            "tinycomputer lab note {run}\nWritten by a Jev intent flow.\nSecond line: each step was grounded on the live screen.",
        ],
        check: Check::NoteNamed("tinycomputer lab note {run}"),
        reset: &[],
    },
    Scenario {
        name: "finder",
        app: "Finder",
        brief: include_str!("../../../scenarios/finder/brief.md"),
        flow: include_str!("../../../scenarios/finder/flow.json"),
        goal: "Make sure a folder named tinycomputer-lab exists on the Desktop, creating it if needed.",
        texts: &["tinycomputer-lab"],
        check: Check::DesktopFolder("tinycomputer-lab"),
        reset: &[Reset::RemoveEmptyDesktopFolder("tinycomputer-lab")],
    },
    Scenario {
        name: "settings-appearance",
        app: "System Settings",
        brief: include_str!("../../../scenarios/settings-appearance/brief.md"),
        flow: include_str!("../../../scenarios/settings-appearance/flow.json"),
        goal: "Open the Appearance settings and leave the current appearance mode visible.",
        texts: &[],
        check: Check::AppearanceRead,
        reset: &[],
    },
    Scenario {
        name: "mail-compose",
        app: "Mail",
        brief: include_str!("../../../scenarios/mail-compose/brief.md"),
        flow: include_str!("../../../scenarios/mail-compose/flow.json"),
        goal: "Write a new email to sam@example.com with the supplied subject and body, and stop before sending it.",
        texts: &[
            "sam@example.com",
            "Moving Thursday's sync to Friday",
            "Hi Sam,\n\nSomething came up on Thursday afternoon. Could we move our weekly sync to Friday at 3pm instead? Same agenda: the launch checklist and the open hiring loops.\n\nIf Friday doesn't work, Monday morning is also free on my side.\n\nThanks,\nAlex",
        ],
        check: Check::MailDraft {
            subject: "Friday",
            body: "3pm",
        },
        // A trial that already produced a compose window otherwise leaves it
        // open, unsent, for the next trial: if that run then fails before
        // ever touching the UI, the checker still reads this stale draft's
        // "Friday"/"3pm" text and records a false pass. `outgoing message` is
        // Mail's own class for both a new draft and an in-progress reply, so
        // this closes both scenarios' leftover windows regardless of title.
        reset: &[Reset::AppleScript(
            r#"tell application "Mail" to delete every outgoing message"#,
        )],
    },
    Scenario {
        name: "mail-reply",
        app: "Mail",
        brief: include_str!("../../../scenarios/mail-reply/brief.md"),
        flow: include_str!("../../../scenarios/mail-reply/flow.json"),
        goal: "Open the newest Inbox message, start a reply, type the supplied text, and stop before sending.",
        texts: &[
            "Thanks for your note. I have read it and will follow up properly by tomorrow.\n\nBest,\nAlex",
        ],
        check: Check::MailReplyDraft,
        reset: &[Reset::AppleScript(
            r#"tell application "Mail" to delete every outgoing message"#,
        )],
    },
    Scenario {
        name: "spotify",
        app: "Spotify",
        brief: include_str!("../../../scenarios/spotify/brief.md"),
        flow: include_str!("../../../scenarios/spotify/flow.json"),
        goal: "Open Liked Songs and make sure a song is playing; choose DONE if one already is.",
        texts: &[],
        check: Check::ShowsPause,
        reset: &[],
    },
];
