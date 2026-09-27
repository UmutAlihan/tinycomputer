//! Tests for action consequences and payment detection.
//!
//! These guard the one promise the unified agent makes unconditionally: it
//! never pays and never does something irreversible without being told to.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    Consequence, FieldHint, consequence, human_needed, payment_evidence, screen_payment_evidence,
};
use crate::surface::{Candidate, Screen};

#[test]
fn payment_controls_are_recognised_in_any_wording() {
    for label in [
        "Pay ₹6,840",
        "PAY NOW",
        "Proceed to payment",
        "Buy now",
        "Place order",
        "Checkout",
        "Complete purchase",
        "Confirm and pay",
        "Subscribe",
    ] {
        assert_eq!(consequence(label), Consequence::Payment, "{label}");
    }
}

#[test]
fn irreversible_controls_need_approval() {
    for label in [
        "Send",
        "Delete draft",
        "Publish",
        "Confirm booking",
        "Cancel reservation",
        "Sign out",
        "Empty Trash",
        "",
        "  --  ",
    ] {
        assert_eq!(consequence(label), Consequence::Irreversible, "{label:?}");
    }
}

#[test]
fn stepping_through_a_booking_is_reversible() {
    for label in [
        "Book",
        "Select",
        "Continue",
        "Search flights",
        "Continue to traveller details",
        "Skip seat selection",
        "No thanks",
        "Sender name",
        "Payday deals",
        "Postcode",
    ] {
        assert_eq!(consequence(label), Consequence::Reversible, "{label}");
    }
}

#[test]
fn consequences_are_ordered_by_severity() {
    assert!(Consequence::Payment > Consequence::Irreversible);
    assert!(Consequence::Irreversible > Consequence::Reversible);
}

#[test]
fn a_card_field_alone_marks_a_payment_page() {
    let by_autocomplete = FieldHint {
        autocomplete: Some("section-pay cc-csc".to_owned()),
        ..FieldHint::default()
    };
    let evidence = payment_evidence("https://ota.test/step/4", &[by_autocomplete], &[]).unwrap();
    assert!(evidence.reasons[0].contains("cc-csc"), "{evidence:?}");

    let by_label = FieldHint {
        label: "Card Number".to_owned(),
        ..FieldHint::default()
    };
    assert!(payment_evidence("https://ota.test/", &[by_label], &[]).is_some());

    let by_name = FieldHint {
        label: "Enter".to_owned(),
        name: Some("card_verification".to_owned()),
        ..FieldHint::default()
    };
    let named = payment_evidence("https://ota.test/", &[by_name], &[]).unwrap();
    assert!(named.reasons[0].contains("card verification"));
}

#[test]
fn a_payment_url_needs_a_payment_control_too() {
    assert!(payment_evidence("https://ota.test/checkout/review", &[], &["Continue"]).is_none());
    let evidence = payment_evidence(
        "https://ota.test/checkout/review",
        &[],
        &["Continue", "Pay now"],
    )
    .unwrap();
    assert!(evidence.reasons[0].contains("Pay now"));
    assert!(payment_evidence("https://ota.test/deals", &[], &["Pay now"]).is_none());
    assert!(payment_evidence("ota.test/payment", &[], &["Pay"]).is_some());
}

#[test]
fn a_traveller_form_is_not_a_payment_page() {
    let fields = [
        FieldHint {
            label: "First name".to_owned(),
            autocomplete: Some("given-name".to_owned()),
            ..FieldHint::default()
        },
        FieldHint {
            label: "Email".to_owned(),
            autocomplete: Some("email".to_owned()),
            name: Some("contact-email".to_owned()),
        },
        FieldHint {
            label: "Card holder's discount code".to_owned(),
            ..FieldHint::default()
        },
    ];
    assert!(
        payment_evidence("https://ota.test/traveller-details", &fields, &["Continue"]).is_none()
    );
}

fn screen_of(candidates: Vec<Candidate>, context: &[&str]) -> Screen {
    Screen {
        app: "browser".to_owned(),
        window: None,
        surface: "window".to_owned(),
        candidates,
        context: context.iter().map(|text| (*text).to_owned()).collect(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    }
}

fn control(role: &str, name: &str, actions: &[&str]) -> Candidate {
    Candidate {
        role: role.to_owned(),
        name: Some(name.to_owned()),
        available_actions: actions.iter().map(|action| (*action).to_owned()).collect(),
        ..Candidate::default()
    }
}

#[test]
fn a_card_input_on_either_surface_marks_a_payment_screen() {
    for field in [
        control("textbox", "Card number", &["Click", "SetValue"]),
        control("textfield", "CVV", &["TypeText"]),
        control("spinbutton", "Expiry date", &[]),
    ] {
        let screen = screen_of(vec![field.clone()], &[]);
        let evidence = screen_payment_evidence(&screen).unwrap_or_else(|| panic!("{field:?}"));
        assert_eq!(evidence.reasons.len(), 1, "one field, one reason");
    }
    let mut beside = screen_of(vec![unlabelled_input(11)], &[]);
    beside.text_nodes = vec![text("Enter the CVV on the back of your card", 10)];
    let evidence = screen_payment_evidence(&beside).unwrap();
    assert!(evidence.reasons[0].contains("cvv"), "{evidence:?}");
}

fn unlabelled_input(order: usize) -> Candidate {
    Candidate {
        role: "textbox".to_owned(),
        order,
        ..Candidate::default()
    }
}

fn text(name: &str, order: usize) -> Candidate {
    Candidate {
        role: "statictext".to_owned(),
        name: Some(name.to_owned()),
        order,
        ..Candidate::default()
    }
}

#[test]
fn a_card_label_alone_marks_a_payment_screen_without_input_metadata() {
    // Some engines report neither a role nor actions for a node; nothing
    // then says it is not a field, so its card label is enough on its own.
    let bare = Candidate {
        name: Some("Card number".to_owned()),
        ..Candidate::default()
    };
    let evidence = screen_payment_evidence(&screen_of(vec![bare], &[])).unwrap();
    assert!(evidence.reasons[0].contains("card number"), "{evidence:?}");
    // A label reported as ref-bearing static text still labels the
    // unlabelled field next to it.
    let label = Candidate {
        ref_id: "e4".to_owned(),
        ..text("Card number", 4)
    };
    let form = screen_of(vec![label, unlabelled_input(5)], &[]);
    assert!(screen_payment_evidence(&form).is_some());
}

#[test]
fn card_wording_counts_only_beside_a_field() {
    let search = || control("textbox", "Search help", &["SetValue"]);
    let mut faq = screen_of(vec![search()], &["Where do I find my CVV?"]);
    assert!(
        screen_payment_evidence(&faq).is_none(),
        "unplaced text is no evidence a field collects a card"
    );
    faq.candidates[0].order = 2;
    faq.text_nodes = vec![text("Where do I find my CVV?", 90)];
    assert!(
        screen_payment_evidence(&faq).is_none(),
        "a footer far from the only field does not label it"
    );
    faq.text_nodes.push(text("CVV", 4));
    assert!(screen_payment_evidence(&faq).is_some());
}

#[test]
fn a_separately_labelled_upi_field_is_still_payment_evidence() {
    // A UPI collect form: an unnamed input with its label as a nearby text
    // node, exactly the shape `CARD_FIELDS` already recognizes when the
    // label sits on the input itself — this proves the same wording is not
    // lost when the label is a separate node beside an unnamed field.
    let mut screen = screen_of(vec![unlabelled_input(4)], &[]);
    screen.text_nodes = vec![text("UPI ID", 3)];
    assert!(
        screen_payment_evidence(&screen).is_some(),
        "a UPI ID label beside a field is not a promotional phrase"
    );
    let mut vpa = screen_of(vec![unlabelled_input(4)], &[]);
    vpa.text_nodes = vec![text("VPA", 3)];
    assert!(screen_payment_evidence(&vpa).is_some());
}

#[test]
fn card_promotions_on_a_home_page_are_not_a_payment_screen() {
    let links = screen_of(
        vec![
            control("link", "IndiGo credit card", &["Click"]),
            control("button", "Accept Essential Only", &["Click"]),
        ],
        &["Save 10% with your debit card", "Card number"],
    );
    assert!(
        screen_payment_evidence(&links).is_none(),
        "no field takes input, so nothing here can collect a card"
    );
    let search = screen_of(
        vec![control("textbox", "Where to?", &["SetValue"])],
        &["Pay less with HDFC credit card", "UPI offers"],
    );
    assert!(
        screen_payment_evidence(&search).is_none(),
        "promotional card wording beside a search box is not a card form"
    );
}

#[test]
fn walls_only_a_person_can_pass_are_named() {
    let needs = |text: &str| human_needed(&[text.to_owned()]);
    assert_eq!(
        needs("Please complete the reCAPTCHA").as_deref(),
        Some("solve the captcha")
    );
    assert_eq!(
        needs("I'm not a robot").as_deref(),
        Some("prove you are human")
    );
    assert_eq!(
        needs("Enter the OTP sent to +91…").as_deref(),
        Some("enter the one-time password")
    );
    assert_eq!(
        needs("Two-factor authentication").as_deref(),
        Some("complete two-factor authentication")
    );
    assert_eq!(needs("Sign in to continue").as_deref(), Some("sign in"));
    assert_eq!(
        needs("Sign in"),
        None,
        "a sign-in link on an ordinary page is no wall"
    );
    assert_eq!(needs("Verification complete"), None);
    assert_eq!(human_needed(&[]), None);
}
