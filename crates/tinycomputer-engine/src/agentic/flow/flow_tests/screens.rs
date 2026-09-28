//! The screens the simulated app shows beyond the mail app: a shop's
//! checkout pages, a booking widget, city rows, result cards, and overlays.

use super::*;

/// The shop's pages.
const EXTRAS: &str = "https://shop.test/extras";

const TERMS: &str = "https://shop.test/terms";

const REVIEW: &str = "https://shop.test/review";

/// The shop's two extras.
const INSURANCE: &str = "Travel insurance";

const PROTECTION: &str = "Seat protection";

/// The shop's screen: an extras page with two checkboxes, a terms link, and
/// Continue; the terms and review pages beyond it.
fn shop_screen(sim: &Sim) -> Screen {
    let page = sim.page().unwrap_or(EXTRAS);
    let (window, candidates, context) = match page {
        TERMS => (
            "Insurance terms",
            vec![node(
                "Download terms",
                "link",
                &["Click"],
                &["main \"Terms\""],
                100.0,
            )],
            vec!["Terms and conditions of travel insurance".to_owned()],
        ),
        REVIEW => (
            "Review",
            vec![node(
                "Pay",
                "button",
                &["Click"],
                &["main \"Review\""],
                100.0,
            )],
            vec!["Review your booking".to_owned()],
        ),
        _ => {
            let checkbox = |name: &str, checked: bool, y: f64| {
                let mut box_node = node(name, "checkbox", &["Click"], &["form \"Extras\""], y);
                if checked {
                    box_node.states = vec!["checked".to_owned()];
                }
                box_node
            };
            (
                "Extras",
                vec![
                    checkbox(PROTECTION, sim.checked.contains(PROTECTION), 100.0),
                    checkbox(INSURANCE, sim.checked.contains(INSURANCE), 140.0),
                    node(
                        "Insurance terms",
                        "link",
                        &["Click"],
                        &["form \"Extras\""],
                        180.0,
                    ),
                    node(
                        "Continue",
                        "button",
                        &["Click"],
                        &["form \"Extras\""],
                        260.0,
                    ),
                ],
                vec!["Choose your extras".to_owned()],
            )
        }
    };
    Screen {
        app: "browser".to_owned(),
        window: Some(window.to_owned()),
        surface: "window".to_owned(),
        candidates,
        context,
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    }
}

/// What pressing `name` does in the shop.
fn press_shop(sim: &mut Sim, name: &str) {
    match name {
        PROTECTION | INSURANCE => {
            let extra = if name == PROTECTION {
                PROTECTION
            } else {
                INSURANCE
            };
            if !sim.checked.remove(extra) {
                sim.checked.insert(extra);
            }
        }
        "Insurance terms" => sim.pages.push(TERMS),
        "Continue" => sim.pages.push(REVIEW),
        _ => {}
    }
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// A booking form: a destination box that opens a search field, as an
/// autocomplete does, and a departure date picked only from a calendar.
#[derive(Debug, Default)]
struct Booking {
    /// Whether the destination's search field is open.
    searching: bool,
    /// `Some(month)` while the calendar is open on that month (0 = January).
    calendar: Option<usize>,
}

/// What pressing `name` does to the booking form.
fn press_booking(sim: &mut Sim, name: &str) {
    let Some(booking) = sim.booking.as_mut() else {
        return;
    };
    match name {
        "Going to?" => booking.searching = true,
        // Choosing a suggestion closes the list over the chosen city.
        "Srinagar, SXR" if booking.searching => {
            booking.searching = false;
            sim.fields
                .insert("Destination".to_owned(), "Srinagar, SXR".to_owned());
        }
        "Departure" => booking.calendar = Some(8),
        "Next Month" => booking.calendar = booking.calendar.map(|month| (month + 1) % 12),
        // The calendar's own aggregated-label container also ends with
        // " 2026" (it lists every day of the open month), so this requires
        // the exact "<day> <month> 2026" shape a single day button carries:
        // a bug that let production code ground and press that container
        // instead of a day must not be able to pass this simulated test.
        day if booking.calendar.is_some() && is_single_day_label(day) => {
            booking.calendar = None;
            sim.fields.insert("Departure".to_owned(), day.to_owned());
        }
        _ => {}
    }
}

/// Emirates' passengers box: a button that does not show its count, and
/// steppers whose labels both name the count they would change.
fn passenger_steppers(adults: u8, root: &str, candidates: &mut Vec<Candidate>) {
    let path = [root, "group \"Passengers\""];
    candidates.push(node("Passengers", "button", &["Click"], &path, 250.0));
    for (verb, y) in [("Decrease", 260.0), ("Increase", 270.0)] {
        candidates.push(node(
            &format!("{verb} number of Adult passengers. You have selected {adults} Adult"),
            "button",
            &["Click"],
            &path,
            y,
        ));
    }
}

/// Trip-type tabs, `selected` marked as the page marks it.
fn trip_tabs(selected: &str, root: &str, candidates: &mut Vec<Candidate>) {
    for (index, tab) in ["Return", "One way", "Multi-city"].into_iter().enumerate() {
        let mut tab_node = node(
            tab,
            "tab",
            &["Click"],
            &[root, "tablist \"Trip\""],
            280.0 + f64::from(u8::try_from(index).unwrap()),
        );
        if tab == selected {
            tab_node.states = vec!["selected".to_owned()];
        }
        candidates.push(tab_node);
    }
}

/// The preselected fare radio, when the simulator shows one.
fn checked_fare(sim: &Sim, root: &str, candidates: &mut Vec<Candidate>) {
    if let Some(fare) = sim.checked_fare {
        let mut radio = node(fare, "radio", &["Click"], &[root, "group \"Fares\""], 200.0);
        radio.states = vec!["checked".to_owned()];
        candidates.push(radio);
    }
}

/// Whether `name` is exactly a single day button's label: `"<day> <month>
/// 2026"`, nothing more. The calendar's own container control names every
/// visible day, so a plain `ends_with(" 2026")` check would also treat
/// pressing that container as picking a day.
fn is_single_day_label(name: &str) -> bool {
    let mut words = name.split(' ');
    let day_is_a_number = words.next().is_some_and(|day| day.parse::<u8>().is_ok());
    day_is_a_number
        && words.next().is_some()
        && words.next() == Some("2026")
        && words.next().is_none()
}

/// The booking form's controls, as they stand.
fn booking_widget(sim: &Sim, booking: &Booking, root: &str, candidates: &mut Vec<Candidate>) {
    let widget = [root, "group \"Booking\""];
    candidates.push(node("Going to?", "button", &["Click"], &widget, 80.0));
    // The destination's own container names its recent searches, so it
    // mentions the option without being it; pressing it chooses nothing.
    candidates.push(node(
        "destinationCity Empty RECENT SEARCHES Srinagar Srinagar International Airport SXR \
         POPULAR DESTINATIONS Mumbai Chhatrapati Shivaji Maharaj International Airport BOM",
        "button",
        &["Click"],
        &widget,
        81.0,
    ));
    if booking.searching {
        // A suggestion row that claims to take text but does not, as
        // IndiGo's comboboxes do.
        candidates.push(node(
            "Mumbai, BOM",
            "combobox",
            &["Click", "SetValue"],
            &widget,
            85.0,
        ));
        let typed = sim.fields.get("Search city").cloned().unwrap_or_default();
        // The box shows what was typed, so it "mentions" the option too.
        let mut search = node(
            "Search city",
            "textbox",
            &["Click", "SetValue"],
            &widget,
            90.0,
        );
        search.value = Some(json!(typed));
        candidates.push(search);
        if !typed.is_empty() && "srinagar".starts_with(&typed.to_lowercase()) {
            candidates.push(node("Srinagar, SXR", "option", &["Click"], &widget, 95.0));
        }
    }
    candidates.push(node("Departure", "button", &["Click"], &widget, 120.0));
    if let Some(month) = booking.calendar {
        // The date field's own label lists the whole open calendar.
        let listing = (1..=28)
            .map(|day| format!("{day} {} 2026", MONTH_NAMES[month]))
            .collect::<Vec<_>>()
            .join(" ");
        candidates.push(node(
            &format!("departureDate Previous Month Next Month {listing}"),
            "button",
            &["Click"],
            &widget,
            125.0,
        ));
        candidates.push(node("Next Month", "button", &["Click"], &widget, 130.0));
        for day in 1..=28 {
            let name = format!("{day} {} 2026", MONTH_NAMES[month]);
            candidates.push(node(&name, "button", &["Click"], &widget, 140.0));
        }
    }
}

/// The obstacle sheet's controls: two buttons, and a checkbox holding a
/// value, to check that value-visibility policy is honored when it is
/// offered as a dismissal option.
fn obstacle_sheet(candidates: &mut Vec<Candidate>) {
    candidates.push(node(
        "Delete Draft",
        "button",
        &["Click"],
        &["sheet"],
        500.0,
    ));
    candidates.push(node(
        "Keep Editing",
        "button",
        &["Click"],
        &["sheet"],
        500.0,
    ));
    candidates.push(Candidate {
        value: Some(json!("unsaved-draft-42")),
        ..node("Remember", "checkbox", &["Click"], &["sheet"], 500.0)
    });
}

/// A city list whose unnamed rows each hold their city as a value and take
/// no text, above the one real search field.
fn city_rows(root: &str, candidates: &mut Vec<Candidate>) {
    for (index, city) in ["Mumbai", "Pune", "Chennai"].into_iter().enumerate() {
        candidates.push(Candidate {
            ref_id: format!("@s:city-{index}"),
            role: "combobox".to_owned(),
            value: Some(json!(city)),
            available_actions: vec!["Click".to_owned(), "SetValue".to_owned()],
            path: vec![root.to_owned(), "list \"Cities\"".to_owned()],
            ..Candidate::default()
        });
    }
    candidates.push(node(
        "Search",
        "textfield",
        &["SetValue"],
        &[root, "group \"Where to\""],
        210.0,
    ));
}

/// The simulator's result list: each card's text as ref-less nodes, and its
/// "Select" button among `candidates`, under an ordinal-labelled list item.
fn result_cards(sim: &Sim, root: &str, candidates: &mut Vec<Candidate>) -> Vec<Candidate> {
    let mut text_nodes = Vec::new();
    for (index, (airline, price, departure)) in sim.results.iter().enumerate() {
        let card = format!("listitem #{}", index + 1);
        let path = vec![root.to_owned(), "list \"Results\"".to_owned(), card];
        let order = 1_000 + index * 10;
        for (offset, text) in [airline, price, departure].into_iter().enumerate() {
            text_nodes.push(Candidate {
                role: "text".to_owned(),
                value: Some(json!(text)),
                path: path.clone(),
                order: order + offset,
                ..Candidate::default()
            });
        }
        candidates.push(Candidate {
            ref_id: format!("@s:select-{}", index + 1),
            role: "button".to_owned(),
            name: Some("Select".to_owned()),
            available_actions: vec!["Click".to_owned()],
            path,
            order: order + 5,
            ..Candidate::default()
        });
    }
    for day in 0..sim.date_strip {
        let path = vec![
            root.to_owned(),
            "list \"Dates\"".to_owned(),
            format!("listitem #{}", day + 1),
        ];
        text_nodes.push(Candidate {
            role: "text".to_owned(),
            value: Some(json!("--")),
            path: path.clone(),
            order: 500 + day * 10,
            ..Candidate::default()
        });
        candidates.push(Candidate {
            ref_id: format!("@s:day-{}", day + 1),
            role: "button".to_owned(),
            name: Some(format!("Please Select Date for {} Oct", day + 12)),
            available_actions: vec!["Click".to_owned()],
            path,
            order: 500 + day * 10 + 5,
            ..Candidate::default()
        });
    }
    text_nodes
}

/// What lies over the simulated page: a promo toast, or something that
/// covers the New Message button.
fn overlays(sim: &Sim, root: &str, candidates: &mut Vec<Candidate>) {
    if sim.has(Quirk::Covered) {
        for candidate in candidates.iter_mut() {
            if candidate.name.as_deref() == Some("New Message") {
                candidate.states = vec!["covered".to_owned()];
            }
        }
    }
    if sim.has(Quirk::PromoToast) {
        let toast = [root, "region \"Unlimited date changes\""];
        candidates.push(node("Close", "button", &["Click"], &toast, 700.0));
        candidates.push(node("Learn more", "link", &["Click"], &toast, 720.0));
    }
}
