//! GNOME device inspector with strict, explicit authorization controls.
//!
//! Overview is concise; Advanced contains detailed reported USB metadata.
//! All write actions require independent confirmation and fresh USBGuard checks.

use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;

use crate::authorization::{self, AllowResult};
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
