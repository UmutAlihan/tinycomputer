//! `Sim`, a tiny stateful application, and `App`, the `AgentBackend` that
//! serves it: it holds field values, follows pages, and records every press.

use super::*;

/// Fixed behaviours a test gives the simulated app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Quirk {
    /// Set-value on the body is silently ignored, as in a rich-text editor.
    BodyIgnoresSetValue,
    /// Every extra row sits in one list instead of three.
    OneRegion,
    /// No action changes anything.
    Frozen,
    /// Observation fails.
    FailObserve,
    /// Launching fails.
    FailLaunch,
    /// The surface has no addresses, as a desktop application has none.
    NoAddresses,
    /// The compose fields sit in a subtree the budgeted snapshot cut short.
    HiddenEditor,
    /// A side drawer lies over the page: every click is refused as covered
    /// until Escape closes it.
    Drawer,
    /// Every click is refused: the element is not visible.
    Unclickable,
    /// A list of cities whose rows carry a `combobox` role but take no
    /// text, above the one real search field.
    CityRows,
    /// The shop's history is stuck: going back or loading an address
    /// reports success and changes nothing.
    StuckHistory,
    /// The Archive button shows disabled.
    DisabledArchive,
    /// A promo toast with a Close button sits over the page until closed.
    PromoToast,
    /// Text typed with no target lands at the end of the field typed into
    /// last, as a browser keeps the focus there.
    FocusStays,
    /// Something Escape does not close covers the New Message button.
    Covered,
}

#[derive(Debug, Default)]
struct Sim {
    compose_open: bool,
    sent: bool,
    obstacle: bool,
    fields: BTreeMap<String, String>,
    presses: Vec<String>,
    clicks: Vec<String>,
    launched: Vec<String>,
    navigated: Vec<String>,
    /// Result cards: (airline, price, departure), shown as a list.
    results: Vec<(&'static str, &'static str, &'static str)>,
    /// Refs of the result cards' "Select" buttons clicked, in order.
    picked: Vec<String>,
    /// Days in a date strip above the results, a longer list than they are.
    date_strip: usize,
    extra_buttons: usize,
    /// A booking form with an autocomplete destination and a calendar.
    booking: Option<Booking>,
    /// A fare radio shown already checked, as a fare page preselects one.
    checked_fare: Option<&'static str>,
    /// A line of guidance shown on the page, such as a date layout.
    hint: Option<&'static str>,
    /// A passengers box with adult steppers, holding this many adults.
    adults: Option<u8>,
    /// Trip-type tabs: the selected one, and how many clicks on the others
    /// the page ignores first, as a page still loading its scripts does.
    trip: Option<(&'static str, u8)>,
    /// A web shop's page stack, newest last; empty for the mail app.
    pages: Vec<&'static str>,
    /// The shop's extras checked: "Travel insurance", "Seat protection".
    checked: BTreeSet<&'static str>,
    /// Times the shop went back a page.
    backs: u32,
    /// The field typed or pasted into last: where the focus stays.
    focused: Option<String>,
    quirks: BTreeSet<Quirk>,
}

impl Sim {
    fn has(&self, quirk: Quirk) -> bool {
        self.quirks.contains(&quirk)
    }

    /// The shop page showing, or `None` for the mail app.
    fn page(&self) -> Option<&'static str> {
        self.pages.last().copied()
    }

    /// A reply carrying the shop's address, as a browser's does.
    fn located(&self, command: &str) -> DesktopResponse {
        DesktopResponse::ok(
            command,
            self.page()
                .map_or_else(|| json!({}), |page| json!({"url": page})),
        )
    }
}

#[derive(Clone, Default)]
struct App(Arc<Mutex<Sim>>);

impl App {
    fn with(configure: impl FnOnce(&mut Sim)) -> Self {
        let app = Self::default();
        configure(&mut app.0.lock().unwrap());
        app
    }

    fn quirky(quirk: Quirk) -> Self {
        Self::with(|sim| {
            sim.quirks.insert(quirk);
        })
    }

    fn sim(&self) -> std::sync::MutexGuard<'_, Sim> {
        self.0.lock().unwrap()
    }
}

fn node(name: &str, role: &str, actions: &[&str], path: &[&str], y: f64) -> Candidate {
    Candidate {
        ref_id: format!("@s:{name}"),
        role: role.to_owned(),
        name: Some(name.to_owned()),
        available_actions: actions.iter().map(|action| (*action).to_owned()).collect(),
        bounds: Some(json!({"x": 10.0, "y": y})),
        path: path.iter().map(|label| (*label).to_owned()).collect(),
        ..Candidate::default()
    }
}

impl App {
    fn screen(&self) -> Screen {
        let sim = self.sim();
        if sim.page().is_some() {
            return shop_screen(&sim);
        }
        let window = if sim.compose_open {
            "New Message"
        } else {
            "Inbox"
        };
        let root = format!("window {window:?}");
        let mut candidates = Vec::new();
        if sim.compose_open {
            for (index, field) in ["To", "Subject"].iter().enumerate() {
                let mut field_node = node(
                    field,
                    "textfield",
                    &["SetValue"],
                    &[&root, "group \"Header\""],
                    100.0 + 30.0 * f64::from(u8::try_from(index).unwrap()),
                );
                field_node.value = sim.fields.get(*field).map(|value| json!(value));
                candidates.push(field_node);
            }
            let mut body = node("Body", "textarea", &["SetValue"], &[&root], 300.0);
            body.value = sim.fields.get("Body").map(|value| json!(value));
            candidates.push(body);
            candidates.push(node(
                "Send",
                "button",
                &["Click"],
                &[&root, "toolbar"],
                40.0,
            ));
        } else {
            checked_fare(&sim, &root, &mut candidates);
            candidates.push(node(
                "New Message",
                "button",
                &["Click"],
                &[&root, "toolbar"],
                40.0,
            ));
            let mut archive = node("Archive", "button", &["Click"], &[&root, "toolbar"], 40.0);
            if sim.has(Quirk::DisabledArchive) {
                archive.states = vec!["disabled".to_owned()];
            }
            candidates.push(archive);
            for index in 0..sim.extra_buttons {
                let region = if sim.has(Quirk::OneRegion) {
                    "list \"Messages\"".to_owned()
                } else {
                    format!("list \"Region {}\"", index % 3)
                };
                let name = format!("Message {index}");
                candidates.push(node(
                    &name,
                    "row",
                    &["Click"],
                    &[&root, &region],
                    60.0 + f64::from(u32::try_from(index).unwrap()),
                ));
            }
        }
        if let Some(booking) = &sim.booking {
            booking_widget(&sim, booking, &root, &mut candidates);
        }
        if let Some(adults) = sim.adults {
            passenger_steppers(adults, &root, &mut candidates);
        }
        if let Some((selected, _)) = sim.trip {
            trip_tabs(selected, &root, &mut candidates);
        }
        if sim.has(Quirk::CityRows) {
            city_rows(&root, &mut candidates);
        }
        overlays(&sim, &root, &mut candidates);
        let text_nodes = result_cards(&sim, &root, &mut candidates);
        let mut surface = "window".to_owned();
        if sim.obstacle {
            surface = "sheet".to_owned();
            obstacle_sheet(&mut candidates);
        }
        Screen {
            app: "Mail".to_owned(),
            window: Some(window.to_owned()),
            surface,
            candidates,
            context: std::iter::once(format!("{window} heading"))
                .chain(sim.hint.map(str::to_owned))
                .collect(),
            unexplored: Vec::new(),
            text_nodes,
        }
    }
}

impl AgentBackend for App {
    fn observe(
        &self,
        _app: &str,
        root: Option<&str>,
        _depth: Depth,
    ) -> Result<Screen, Box<DesktopResponse>> {
        if self.sim().has(Quirk::FailObserve) {
            return Err(Box::new(DesktopResponse::err(
                "snapshot",
                tinycomputer_bus::DesktopError::new("APP_NOT_FOUND", "no such app"),
            )));
        }
        let mut screen = self.screen();
        if self.sim().has(Quirk::HiddenEditor) && self.sim().compose_open {
            let (fields, rest): (Vec<_>, Vec<_>) =
                screen.candidates.into_iter().partition(|candidate| {
                    candidate
                        .available_actions
                        .iter()
                        .any(|action| action == "SetValue")
                });
            if root == Some("@s:editor") {
                screen.candidates = fields;
            } else {
                screen.candidates = rest;
                screen.unexplored = vec!["@s:editor".to_owned()];
            }
        }
        Ok(screen)
    }

    fn execute(
        &self,
        operation: JevOperation,
        target: Option<Candidate>,
        text: Option<String>,
    ) -> DesktopResponse {
        let mut sim = self.sim();
        if sim.has(Quirk::Frozen) {
            return DesktopResponse::ok("fake", json!({}));
        }
        let name = target
            .as_ref()
            .and_then(|target| target.name.clone())
            .unwrap_or_default();
        if sim.has(Quirk::Unclickable) && operation == JevOperation::Click {
            return DesktopResponse::err(
                "click",
                tinycomputer_bus::DesktopError::new(
                    "NOT_ACTIONABLE",
                    "Element exists but is not visible.",
                ),
            );
        }
        if sim.has(Quirk::Drawer) && operation == JevOperation::Click {
            return DesktopResponse::err(
                "click",
                tinycomputer_bus::DesktopError::new(
                    "NOT_ACTIONABLE",
                    format!("Element '@s:{name}' is covered by <div.drawer> at its click point"),
                ),
            );
        }
        if is_city_row(target.as_ref()) && operation == JevOperation::TypeText {
            return not_a_text_field();
        }
        match operation {
            JevOperation::Click => {
                if let Some(reference) = target
                    .as_ref()
                    .map(|target| target.ref_id.clone())
                    .filter(|reference| reference.starts_with("@s:select-"))
                {
                    sim.picked.push(reference);
                }
                sim.clicks.push(name.clone());
                if name == "Close" && sim.has(Quirk::PromoToast) {
                    sim.quirks.remove(&Quirk::PromoToast);
                    return DesktopResponse::ok("click", json!({}));
                }
                if sim.page().is_some() {
                    press_shop(&mut sim, &name);
                    return sim.located("click");
                }
                match name.as_str() {
                    "New Message" => sim.compose_open = true,
                    "Send" => sim.sent = true,
                    "Keep Editing" => sim.obstacle = false,
                    "Archive" => sim.compose_open = false,
                    "Return" | "One way" | "Multi-city" => {
                        if let Some((selected, ignored)) = sim.trip.as_mut() {
                            if *ignored > 0 {
                                *ignored -= 1;
                            } else {
                                *selected = match name.as_str() {
                                    "Return" => "Return",
                                    "One way" => "One way",
                                    _ => "Multi-city",
                                };
                            }
                        }
                    }
                    _ if name.starts_with("Increase number of Adult") => {
                        sim.adults = sim.adults.map(|adults| adults + 1);
                    }
                    _ if name.starts_with("Decrease number of Adult") => {
                        sim.adults = sim.adults.map(|adults| adults.saturating_sub(1));
                    }
                    _ if sim.booking.is_some() => press_booking(&mut sim, &name),
                    _ => {}
                }
            }
            JevOperation::TypeText if name == "Mumbai, BOM" => {}
            JevOperation::TypeText if target.is_none() && sim.has(Quirk::FocusStays) => {
                if let Some(field) = sim.focused.clone() {
                    sim.fields
                        .entry(field)
                        .or_default()
                        .push_str(&text.unwrap_or_default());
                }
            }
            // Text with no target goes to the focused field: the booking
            // form's search box once it is open.
            JevOperation::TypeText if target.is_none() => {
                sim.fields
                    .insert("Search city".to_owned(), text.unwrap_or_default());
            }
            JevOperation::TypeText if !(name == "Body" && sim.has(Quirk::BodyIgnoresSetValue)) => {
                sim.focused = Some(name.clone());
                sim.fields.insert(name, text.unwrap_or_default());
            }
            _ => {}
        }
        DesktopResponse::ok("fake", json!({}))
    }

    fn read_value(&self, target: &Candidate) -> Option<String> {
        let name = target.name.clone().unwrap_or_default();
        Some(self.sim().fields.get(&name).cloned().unwrap_or_default())
    }

    fn paste(&self, _app: &str, target: &Candidate, text: &str) -> DesktopResponse {
        if is_city_row(Some(target)) {
            return not_a_text_field();
        }
        let mut sim = self.sim();
        let name = target.name.clone().unwrap_or_default();
        sim.focused = Some(name.clone());
        sim.fields.insert(name, text.to_owned());
        DesktopResponse::ok("paste", json!({}))
    }

    fn press(&self, _app: &str, combo: &str) -> DesktopResponse {
        let mut sim = self.sim();
        sim.presses.push(combo.to_owned());
        if sim.has(Quirk::Frozen) {
            return DesktopResponse::ok("press", json!({}));
        }
        match combo {
            "cmd+n" => sim.compose_open = true,
            "escape" => {
                sim.obstacle = false;
                sim.quirks.remove(&Quirk::Drawer);
            }
            _ => {}
        }
        DesktopResponse::ok("press", json!({}))
    }

    fn back(&self, _app: &str) -> DesktopResponse {
        let mut sim = self.sim();
        if sim.page().is_none() {
            return DesktopResponse::err(
                "back",
                tinycomputer_bus::DesktopError::new("ACTION_NOT_SUPPORTED", "no history"),
            );
        }
        sim.backs += 1;
        if !sim.has(Quirk::StuckHistory) && sim.pages.len() > 1 {
            sim.pages.pop();
        }
        sim.located("back")
    }

    fn launch(&self, app: &str) -> DesktopResponse {
        let mut sim = self.sim();
        sim.launched.push(app.to_owned());
        if sim.page().is_some() {
            return sim.located("launch");
        }
        if sim.has(Quirk::FailLaunch) {
            return DesktopResponse::err(
                "launch",
                tinycomputer_bus::DesktopError::new("APP_NOT_FOUND", "no such app"),
            );
        }
        DesktopResponse::ok("launch", json!({}))
    }

    fn navigate(&self, url: &str) -> DesktopResponse {
        let mut sim = self.sim();
        if sim.has(Quirk::NoAddresses) {
            return DesktopResponse::err(
                "navigate",
                tinycomputer_bus::DesktopError::new("ACTION_NOT_SUPPORTED", "no addresses"),
            );
        }
        sim.navigated.push(url.to_owned());
        if !sim.has(Quirk::StuckHistory)
            && let Some(at) = sim.pages.iter().position(|page| *page == url)
        {
            sim.pages.truncate(at + 1);
        }
        DesktopResponse::ok("navigate", json!({"url": url, "title": "Flights"}))
    }
}

fn is_city_row(target: Option<&Candidate>) -> bool {
    target.is_some_and(|target| target.ref_id.starts_with("@s:city-"))
}

fn not_a_text_field() -> DesktopResponse {
    DesktopResponse::err(
        "type-text",
        tinycomputer_bus::DesktopError::new("NOT_A_TEXT_FIELD", "no input takes the text"),
    )
}
