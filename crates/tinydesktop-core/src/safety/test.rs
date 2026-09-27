//! Tests for action consequences and payment detection.
//!
//! These guard the one promise the unified agent makes unconditionally: it
//! never pays and never does something irreversible without being told to.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Consequence, FieldHint, consequence, human_needed, payment_evidence};

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
