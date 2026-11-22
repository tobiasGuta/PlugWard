//! GNOME device inspector with strict, explicit authorization controls.
//!
//! Overview is concise; Advanced contains detailed reported USB metadata.
//! All write actions require independent confirmation and fresh USBGuard checks.

use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;

use crate::authorization::{self, AllowResult};
use crate::policy::{self, TrustResult, TrustReceipt};
use crate::usbguard::{self, Device};

pub fn show(parent: &adw::ApplicationWindow, device: &Device) {
    let window = gtk::Window::new();
    window.set_title(Some("PlugWard · Device details"));
    window.set_default_size(560, 660);
    window.set_modal(true);
    window.set_transient_for(Some(parent));

    // Make the header the window's titlebar instead of placing a second
    // header inside its content (GTK otherwise renders two titlebars).
    let header = gtk::HeaderBar::new();
    window.set_titlebar(Some(&header));
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(540);
    scroll.set_child(Some(&clamp));
    root.append(&scroll);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.set_margin_start(20);
    page.set_margin_end(20);
    page.set_margin_top(20);
    page.set_margin_bottom(24);
    clamp.set_child(Some(&page));

    let title = gtk::Label::new(Some(&device.display_name()));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    title.set_wrap(true);
    page.append(&title);

    let note = gtk::Label::new(Some(
        "Device details are reported by USBGuard, not proof of authentic hardware. Any access change requires separate confirmation.",
    ));
    note.add_css_class("dim-label");
    note.set_xalign(0.0);
    note.set_wrap(true);
    page.append(&note);

    let overview = gtk::Box::new(gtk::Orientation::Vertical, 16);
    let advanced = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.add_titled(&overview, Some("overview"), "Overview");
    stack.add_titled(&advanced, Some("advanced"), "Advanced");
    let switcher = gtk::StackSwitcher::new();
    switcher.set_stack(Some(&stack));
    switcher.set_halign(gtk::Align::Center);
    page.append(&switcher);
    page.append(&stack);

    let essentials = adw::PreferencesGroup::new();
    essentials.set_title("Device overview");
    let essentials_row = adw::ActionRow::new();
    essentials_row.set_title(&device.display_name());
    essentials_row.set_subtitle(&format!(
        "USB ID {} · Port {} · {}",
        Device::display_attribute(&device.usb_id),
        Device::display_attribute(&device.port),
        device.connection_label(),
    ));
    essentials.add(&essentials_row);
    overview.append(&essentials);

    let group = adw::PreferencesGroup::new();
    group.set_title("Current authorization (at opening)");
    let status = adw::ActionRow::new();
    status.set_title("USBGuard status");
    status.set_subtitle(device.status());
    group.add(&status);
    let number = adw::ActionRow::new();
    number.set_title("Runtime device number");
    number.set_subtitle(&device.number.to_string());
    group.add(&number);
    overview.append(&group);

    if device.target == "block" {
        let approval = adw::PreferencesGroup::new();
        approval.set_title("Temporary authorization");
        approval.set_description(Some(
            "Allows this connection only, without adding a persistent rule. Use only with a USB device you independently trust.",
        ));
        let action = adw::ActionRow::new();
        action.set_title("Allow this connection once");
        let button = gtk::Button::with_label("Allow once…");
        button.add_css_class("suggested-action");
        match authorization::eligibility(device) {
            Ok(()) => {
                let current = device.clone();
                let parent = window.clone();
                button.connect_clicked(move |_| confirm_allow_once(&parent, current.clone()));
            }
            Err(reason) => {
                button.set_sensitive(false);
                action.set_subtitle(reason);
            }
        }
        action.add_suffix(&button);
        approval.add(&action);
        overview.append(&approval);
    }

    // These actions are intentionally unavailable on built-in components.
    // The action backend also repeats eligibility checks against a fresh list.
    if authorization::block_eligibility(device).is_ok() {
        let block_group = adw::PreferencesGroup::new();
        block_group.set_title("Revoke current access");
        block_group.set_description(Some(
            "Block this connection only. This does not remove an existing permanent allow rule.",
        ));
        let row = adw::ActionRow::new();
        row.set_title("Block this connection");
        let button = gtk::Button::with_label("Block…");
        button.add_css_class("destructive-action");
        let selected = device.clone();
        let parent = window.clone();
        button.connect_clicked(move |_| show_policy_confirmation(&parent, selected.clone(), PolicyAction::Block));
        row.add_suffix(&button);
        block_group.add(&row);
        overview.append(&block_group);
    }

    if let Ok(rule) = policy::scoped_rule(device) {
        let trust_group = adw::PreferencesGroup::new();
        trust_group.set_title("Permanent trust");
        trust_group.set_description(Some(
            "Adds a strict, port-specific persistent rule after confirmation and a user-owned policy backup. Undo is offered after verification.",
        ));
        let row = adw::ActionRow::new();
        row.set_title("Trust this USB storage device across connections");
        row.set_subtitle("Only this serial, USB ID, hash, parent, interface, and port");
        let button = gtk::Button::with_label("Review rule…");
        let selected = device.clone();
        let parent = window.clone();
        button.connect_clicked(move |_| show_policy_confirmation(&parent, selected.clone(), PolicyAction::Trust));
        row.add_suffix(&button);
        trust_group.add(&row);
        overview.append(&trust_group);
        // The rule preview is rendered in the final confirmation, not hidden
        // behind an unsafe button that might apply unknown broad defaults.
        let _ = rule;
    }

    let details = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let heading = gtk::Label::new(Some("Reported device attributes"));
    heading.add_css_class("heading");
    heading.set_xalign(0.0);
    details.append(&heading);
    let technical_note = gtk::Label::new(Some(
        "Advanced details are reported by USBGuard. Serial numbers, hashes, interface classes, and USB identifiers can be spoofed.",
    ));
    technical_note.add_css_class("dim-label");
    technical_note.set_wrap(true);
    technical_note.set_xalign(0.0);
    advanced.append(&technical_note);
    add_field(&details, "USB vendor:product ID", &device.usb_id);
    add_field(&details, "USB-reported name", &device.name);
    add_field(&details, "USB port", &device.port);
    add_field(&details, "Connection type", device.connection_label());
    add_field(&details, "Serial number", &device.serial);
    add_field(&details, "USBGuard hash", &device.hash);
    add_field(&details, "Parent hash", &device.parent_hash);
    add_field(
        &details,
        "USB interface codes",
        &device.interfaces.join(", "),
    );
    advanced.append(&details);

    let verify_group = adw::PreferencesGroup::new();
    verify_group.set_title("Freshness check");
    verify_group.set_description(Some(
        "Recheck compares this snapshot with the latest USBGuard listing. A match is not proof of identity or continuous presence.",
    ));
    overview.append(&verify_group);

    let verify_row = adw::ActionRow::new();
    verify_row.set_title("Compare with current listing");
    let verify_button = gtk::Button::with_label("Recheck");
    verify_button.add_css_class("suggested-action");
    verify_row.add_suffix(&verify_button);
    verify_group.add(&verify_row);

    let result = gtk::Label::new(Some("Not rechecked since opening"));
    result.set_wrap(true);
    result.set_xalign(0.0);
    result.add_css_class("dim-label");
    overview.append(&result);

    let snapshot = device.clone();
    verify_button.connect_clicked(move |button| {
        button.set_sensitive(false);
        result.set_text("Checking with USBGuard…");
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(usbguard::list_devices());
        });

        let expected = snapshot.clone();
        let result = result.clone();
        let button = button.clone();
        glib::timeout_add_local(Duration::from_millis(150), move || {
            match receiver.try_recv() {
                Ok(Ok(devices)) => {
                    match devices.iter().find(|d| d.number == expected.number) {
                        Some(current) if expected.matches_snapshot(current) => result.set_text(
                            "Matches the latest listing (reported metadata only).",
                        ),
                        Some(_) => result.set_text(
                            "Device attributes or authorization changed. Close this window and reopen its details.",
                        ),
                        None => result.set_text(
                            "The original USBGuard device number is no longer present.",
                        ),
                    }
                    button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Ok(Err(error)) => {
                    result.set_text(&format!(
                        "Could not recheck: {}",
                        Device::display_attribute(&error)
                    ));
                    button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(TryRecvError::Disconnected) => {
                    result.set_text("USBGuard query worker disconnected.");
                    button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            }
        });
    });

    window.set_child(Some(&root));
    window.present();
}

fn add_field(container: &gtk::Box, name: &str, value: &str) {
    let field = gtk::Box::new(gtk::Orientation::Vertical, 3);
    field.add_css_class("pw-detail-field");
    let label = gtk::Label::new(Some(name));
    label.add_css_class("dim-label");
    label.add_css_class("caption");
    label.set_xalign(0.0);
    field.append(&label);

    let text = gtk::Label::new(Some(&Device::display_attribute(value)));
    text.set_xalign(0.0);
    text.set_selectable(true);
    text.set_wrap(true);
    text.set_wrap_mode(gtk::pango::WrapMode::Char);
    field.append(&text);
    container.append(&field);
}


fn confirm_allow_once(parent: &gtk::Window, snapshot: Device) {
    let dialog = gtk::Window::builder()
        .title("Confirm temporary USB access")
        .default_width(450)
        .modal(true)
        .transient_for(parent)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_top(22);
    content.set_margin_bottom(22);

    let heading = gtk::Label::new(Some("Allow this USB device once?"));
    heading.add_css_class("title-2");
    heading.set_xalign(0.0);
    heading.set_wrap(true);
    content.append(&heading);

    let info = gtk::Label::new(Some(&format!(
        "Reported device: {}\nUSB ID: {}\nPort: {}\nCurrent state: {}\n\nThis permits the current USB connection only. It does not add a permanent allow rule.\n\nDevice descriptions and hashes can be spoofed. Only proceed if you independently trust the physical device.",
        snapshot.display_name(),
        Device::display_attribute(&snapshot.usb_id),
        Device::display_attribute(&snapshot.port),
        snapshot.status()
    )));
    info.set_xalign(0.0);
    info.set_wrap(true);
    info.set_selectable(true);
    content.append(&info);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    actions.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    let approve = gtk::Button::with_label("Allow once");
    approve.add_css_class("suggested-action");
    actions.append(&cancel);
    actions.append(&approve);
    content.append(&actions);

    let cancel_window = dialog.clone();
    cancel.connect_clicked(move |_| cancel_window.close());

    let confirm_window = dialog.clone();
    let details_window = parent.clone();
    approve.connect_clicked(move |_| {
        // Only this explicit confirmation event can trigger the D-Bus write.
        confirm_window.close();
        run_allow_once(&details_window, snapshot.clone());
    });

    dialog.set_child(Some(&content));
    dialog.present();
}

fn run_allow_once(parent: &gtk::Window, snapshot: Device) {
    let window = gtk::Window::builder()
        .title("USB authorization result")
        .default_width(450)
        .modal(true)
        .transient_for(parent)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_top(22);
    content.set_margin_bottom(22);

    let heading = gtk::Label::new(Some("Checking device and requesting access…"));
    heading.set_wrap(true);
    heading.set_xalign(0.0);
    heading.add_css_class("heading");
    content.append(&heading);

    let description = gtk::Label::new(Some(
        "PlugWard is rechecking the selected device before asking USBGuard to authorize its current connection. The system may request administrator authentication.",
    ));
    description.set_wrap(true);
    description.set_xalign(0.0);
    content.append(&description);

    let close = gtk::Button::with_label("Close");
    close.set_halign(gtk::Align::End);
    close.set_sensitive(false);
    let dialog = window.clone();
    close.connect_clicked(move |_| dialog.close());
    content.append(&close);
    window.set_child(Some(&content));
    window.present();

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(authorization::allow_once(&snapshot));
    });

    glib::timeout_add_local(Duration::from_millis(150), move || {
        match receiver.try_recv() {
            Ok(Ok(AllowResult::Confirmed)) => {
                heading.set_text("Temporary authorization confirmed");
                description.set_text(
                    "USBGuard's latest listing shows this connection as allowed. No permanent rule was requested. Refresh the main dashboard to see the new status.",
                );
                close.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Ok(Ok(AllowResult::Unverified(message))) => {
                heading.set_text("Authorization state not verified");
                description.set_text(&message);
                close.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Ok(Err(error)) => {
                heading.set_text("Authorization not confirmed");
                description.set_text(&Device::display_diagnostic(&error));
                close.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Disconnected) => {
                heading.set_text("Authorization worker unavailable");
                description.set_text(
                    "Unable to determine the result. Inspect USBGuard's current device list before another attempt.",
                );
                close.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
        }
    });
}


#[derive(Clone, Copy)]
enum PolicyAction {
    Block,
    Trust,
}

#[derive(Debug)]
enum PolicyOutcome {
    Blocked(AllowResult),
    Trusted(TrustResult),
}

fn show_policy_confirmation(parent: &gtk::Window, device: Device, action: PolicyAction) {
    let title = match action {
        PolicyAction::Block => "Block this USB connection?",
        PolicyAction::Trust => "Permanently trust this USB storage device?",
    };
    let dialog = gtk::Window::builder()
        .title("Confirm USBGuard policy action")
        .default_width(520)
        .modal(true)
        .transient_for(parent)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 14);
    body.set_margin_start(24);
    body.set_margin_end(24);
    body.set_margin_top(20);
    body.set_margin_bottom(20);

    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("title-2");
    heading.set_xalign(0.0);
    heading.set_wrap(true);
    body.append(&heading);

    let scope = match action {
        PolicyAction::Block => "This blocks the currently connected device only. A pre-existing permanent allow rule may authorize it again after reconnection. Internal devices and non-storage classes are excluded.",
        PolicyAction::Trust => "This stores a persistent USBGuard allow rule, affecting future connections and surviving reboot. USB descriptors and hashes can be spoofed. You must independently trust this physical device.",
    };
    let text = gtk::Label::new(Some(&format!(
        "{}\n\nReported: {}\nUSB ID: {}\nPort: {}\n\n{}",
        scope,
        device.display_name(),
        Device::display_attribute(&device.usb_id),
        Device::display_attribute(&device.port),
        if matches!(action, PolicyAction::Trust) {
            "An on-disk user-owned policy snapshot is required before making this change."
        } else {
            "No existing permanent USBGuard rule will be changed."
        }
    )));
    text.set_wrap(true);
    text.set_xalign(0.0);
    body.append(&text);

    if matches!(action, PolicyAction::Trust) {
        let preview = policy::scoped_rule(&device).unwrap_or_else(|error| format!("Rule unavailable: {error}"));
        let rule = gtk::Label::new(Some(&format!("Exact rule to append:\n{preview}")));
        rule.set_selectable(true);
        rule.set_wrap(true);
        rule.set_wrap_mode(gtk::pango::WrapMode::Char);
        rule.set_xalign(0.0);
        rule.add_css_class("pw-detail-field");
        body.append(&rule);
    }

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    actions.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    let label = match action {
        PolicyAction::Block => "Block connection",
        PolicyAction::Trust => "Save permanent trust rule",
    };
    let confirm = gtk::Button::with_label(label);
    if matches!(action, PolicyAction::Block) {
        confirm.add_css_class("destructive-action");
    } else {
        confirm.add_css_class("suggested-action");
    }
    let cancel_dialog = dialog.clone();
    cancel.connect_clicked(move |_| cancel_dialog.close());
    let confirm_dialog = dialog.clone();
    let parent = parent.clone();
    confirm.connect_clicked(move |_| {
        confirm_dialog.close();
        run_policy_action(&parent, device.clone(), action);
    });
    actions.append(&cancel);
    actions.append(&confirm);
    body.append(&actions);
    dialog.set_child(Some(&body));
    dialog.present();
}

fn run_policy_action(parent: &gtk::Window, device: Device, action: PolicyAction) {
    let window = gtk::Window::builder()
        .title("USBGuard policy result")
        .default_width(520)
        .modal(true)
        .transient_for(parent)
        .build();

    let body = gtk::Box::new(gtk::Orientation::Vertical, 14);
    body.set_margin_start(24);
    body.set_margin_end(24);
    body.set_margin_top(22);
    body.set_margin_bottom(22);
    let heading = gtk::Label::new(Some("Checking device and contacting USBGuard…"));
    heading.add_css_class("heading");
    heading.set_xalign(0.0);
    heading.set_wrap(true);
    body.append(&heading);
    let description = gtk::Label::new(Some(
        "This action may require the system's existing administrator authentication.",
    ));
    description.set_wrap(true);
    description.set_xalign(0.0);
    description.set_selectable(true);
    body.append(&description);

    let action_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    action_box.set_halign(gtk::Align::End);
    let undo = gtk::Button::with_label("Undo this trust…");
    undo.set_visible(false);
    undo.add_css_class("destructive-action");
    let close = gtk::Button::with_label("Close");
    close.set_sensitive(false);
    let close_window = window.clone();
    close.connect_clicked(move |_| close_window.close());
    action_box.append(&undo);
    action_box.append(&close);
    body.append(&action_box);
    window.set_child(Some(&body));
    window.present();

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result = match action {
            PolicyAction::Block => authorization::block_connection(&device).map(PolicyOutcome::Blocked),
            PolicyAction::Trust => policy::trust_permanently(&device).map(PolicyOutcome::Trusted),
        };
        let _ = sender.send(result);
    });

    glib::timeout_add_local(Duration::from_millis(150), move || {
        match receiver.try_recv() {
            Ok(Ok(PolicyOutcome::Blocked(AllowResult::Confirmed))) => {
                heading.set_text("Current connection blocked");
                description.set_text(
                    "USBGuard confirmed the device is blocked for this connection. This has not removed any permanent allow rule.",
                );
                close.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Ok(Ok(PolicyOutcome::Blocked(AllowResult::Unverified(message)))) => {
                heading.set_text("Block outcome not verified");
                description.set_text(&message);
                close.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Ok(Ok(PolicyOutcome::Trusted(TrustResult::Confirmed(receipt)))) => {
                heading.set_text("Permanent rule saved and verified");
                description.set_text(&format!(
                    "Rule #{} is present in USBGuard's policy. User-owned backup: {}\n\nA newly appended rule does not guarantee the current connection was authorized. Reconnect the trusted device to test future matching. Undo can remove only this exact new rule.",
                    receipt.rule_id, receipt.backup_path.display()
                ));
                undo.set_visible(true);
                let current = receipt.clone();
                let parent = window.clone();
                undo.connect_clicked(move |_| confirm_undo(&parent, current.clone()));
                close.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Ok(Ok(PolicyOutcome::Trusted(TrustResult::Unverified(message)))) => {
                heading.set_text("Persistent policy change not verified");
                description.set_text(&message);
                close.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Ok(Err(error)) => {
                heading.set_text("USBGuard rejected or could not verify the action");
                description.set_text(&Device::display_diagnostic(&error));
                close.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Disconnected) => {
                heading.set_text("Result unavailable");
                description.set_text("Inspect USBGuard's current policy and device list before trying again.");
                close.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
        }
    });
}

fn confirm_undo(parent: &gtk::Window, receipt: TrustReceipt) {
    let dialog = gtk::Window::builder()
        .title("Undo permanent trust")
        .default_width(450)
        .modal(true)
        .transient_for(parent)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 14);
    body.set_margin_start(20);
    body.set_margin_end(20);
    body.set_margin_top(20);
    body.set_margin_bottom(20);
    let text = gtk::Label::new(Some(&format!(
        "Remove the exact USBGuard rule #{} created by PlugWard?\n\nThis affects future connections. A currently authorized device may remain active until it is unplugged or separately blocked.",
        receipt.rule_id
    )));
    text.set_xalign(0.0);
    text.set_wrap(true);
    body.append(&text);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    actions.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    let proceed = gtk::Button::with_label("Remove this rule");
    proceed.add_css_class("destructive-action");
    let cancel_dialog = dialog.clone();
    cancel.connect_clicked(move |_| cancel_dialog.close());
    let confirm_dialog = dialog.clone();
    let result_parent = parent.clone();
    proceed.connect_clicked(move |_| {
        confirm_dialog.close();
        run_undo(&result_parent, receipt.clone());
    });
    actions.append(&cancel);
    actions.append(&proceed);
    body.append(&actions);
    dialog.set_child(Some(&body));
    dialog.present();
}

fn run_undo(parent: &gtk::Window, receipt: TrustReceipt) {
    let dialog = gtk::Window::builder()
        .title("Undo trust result")
        .default_width(440)
        .modal(true)
        .transient_for(parent)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_margin_start(20);
    body.set_margin_end(20);
    body.set_margin_top(20);
    body.set_margin_bottom(20);
    let result = gtk::Label::new(Some("Verifying the exact policy rule before removal…"));
    result.set_xalign(0.0);
    result.set_wrap(true);
    body.append(&result);
    let close = gtk::Button::with_label("Close");
    close.set_sensitive(false);
    close.set_halign(gtk::Align::End);
    let close_dialog = dialog.clone();
    close.connect_clicked(move |_| close_dialog.close());
    body.append(&close);
    dialog.set_child(Some(&body));
    dialog.present();

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(policy::undo_trust(&receipt));
    });
    glib::timeout_add_local(Duration::from_millis(150), move || {
        match receiver.try_recv() {
            Ok(Ok(true)) => result.set_text("Exact rule removed. Future connections will be evaluated against the remaining USBGuard policy."),
            Ok(Ok(false)) => result.set_text("USBGuard did not confirm rule removal. Inspect current policy."),
            Ok(Err(error)) => result.set_text(&Device::display_diagnostic(&error)),
            Err(TryRecvError::Disconnected) => result.set_text("Unable to determine removal result."),
            Err(TryRecvError::Empty) => return glib::ControlFlow::Continue,
        }
        close.set_sensitive(true);
        glib::ControlFlow::Break
    });
}
