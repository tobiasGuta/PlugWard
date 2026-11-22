//! GNOME read-only device inspector.
//!
//! Deliberately provides no authorization/write operations. Recheck only
//! compares against a new, privileged-service-supplied device list.

use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;

use crate::usbguard::{self, Device};

pub fn show(parent: &adw::ApplicationWindow, device: &Device) {
    let window = gtk::Window::new();
    window.set_title(Some("PlugWard · Device details"));
    window.set_default_size(560, 660);
    window.set_modal(true);
    window.set_transient_for(Some(parent));

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let header = gtk::HeaderBar::new();
    root.append(&header);

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
        "Read-only snapshot from USBGuard. Device names and IDs can be spoofed; this view does not prove hardware identity.",
    ));
    note.add_css_class("dim-label");
    note.set_xalign(0.0);
    note.set_wrap(true);
    page.append(&note);

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
    page.append(&group);

    let details = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let heading = gtk::Label::new(Some("Reported device attributes"));
    heading.add_css_class("heading");
    heading.set_xalign(0.0);
    details.append(&heading);
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
    page.append(&details);

    let verify_group = adw::PreferencesGroup::new();
    verify_group.set_title("Freshness check");
    verify_group.set_description(Some(
        "Recheck compares this snapshot with the latest USBGuard listing. A match is not proof of identity or continuous presence.",
    ));
    page.append(&verify_group);

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
    page.append(&result);

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
