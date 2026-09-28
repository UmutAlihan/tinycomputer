Read my five most recent WhatsApp chats in the WhatsApp desktop app and
return, for each, the chat's name and its ten most recent messages. Only read:
never type, send, react, delete, archive, or change a setting.

What the app shows, from its accessibility tree:

- The left pane lists chats, most recent first, under filter buttons ("All",
  "Unread", "Favorites", "Groups") and an "Archived" row. The Archived row is
  not a chat; the chats start below it. Each chat is a button named for the
  chat; the chat already open shows instead as text with its last message.
- Opening a chat shows its name as a button above the messages, and its
  messages as a run of text elements, oldest at the top, each like
  "message, <text>, <time>, Received from <name>" or
  "Your message, <text>, <time>, Sent to <name>".
- If the sidebar shows Updates, Calls, or another tab, the "Chats" tab
  brings the chat list back.

- The app bundle's file name starts with an invisible left-to-right mark, so
  it is opened by its bundle id, `net.whatsapp.WhatsApp`; by name, launching
  fails with APP_NOT_FOUND.

Plan: bring WhatsApp forward, show all chats, then five times: pick the chat
nearest the top of the list that is not already collected, read its name,
and extract its messages. `output.json` asks for the answer as five chats,
each with its name and its ten most recent messages.

Run it on this Mac, from a shell with the Accessibility permission:

    FLOW_FILE=crates/tinycomputer-examples/tasks/whatsapp/plan.json \
    TASK_FILE=crates/tinycomputer-examples/tasks/whatsapp/task.md \
    FACTS_FILE=crates/tinycomputer-examples/tasks/whatsapp/facts.json \
    OUTPUT_FILE=crates/tinycomputer-examples/tasks/whatsapp/output.json \
    TASK_SURFACE=desktop TINYCOMPUTER_MODULE="$(scripts/build-module)" \
    cargo run -p tinycomputer-examples --bin task_live

Opening a chat marks it read in WhatsApp, so its senders see read receipts.
