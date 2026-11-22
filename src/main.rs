mod usbguard;
mod details;
mod authorization;
mod activity;
mod security;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;

#[derive(Clone)]
struct View {
    parent: adw::ApplicationWindow,
    activity: Rc<RefCell<activity::ActivityHistory>>,
    connection: gtk::Label,
    connection_icon: gtk::Image,
    total_count: gtk::Label,
    allowed_count: gtk::Label,
    blocked_count: gtk::Label,
    help: gtk::Label,
    device_area: gtk::Box,
    refresh_button: gtk::Button,
    controllers_expanded: Rc<Cell<bool>>,
    last_devices: Rc<RefCell<Option<Vec<usbguard::Device>>>>,
}

fn main() {
    let application = adw::Application::builder()
        .application_id("io.github.tobiasguta.PlugWard")
        .build();
    application.connect_activate(build_window);
    application.run();
}

fn build_window(application: &adw::Application) {
    load_styles();

    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("PlugWard")
        .default_width(900)
        .default_height(740)
        .build();

    let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let header = adw::HeaderBar::new();
    let refresh_button = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh USB devices")
        .build();
    header.pack_end(&refresh_button);

    let activity_button = gtk::Button::with_label("Activity");
    activity_button.set_tooltip_text(Some("USB changes observed during this PlugWard session"));
    header.pack_start(&activity_button);
    let security_button = gtk::Button::with_label("Security");
    security_button.set_tooltip_text(Some("Read-only USBGuard enforcement and boot-status checks"));
    header.pack_start(&security_button);

    layout.append(&header);

    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(880);
    clamp.set_tightening_threshold(640);
    scroll.set_child(Some(&clamp));
    layout.append(&scroll);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 22);
    page.set_margin_start(20);
    page.set_margin_end(20);
    page.set_margin_top(28);
    page.set_margin_bottom(32);
    clamp.set_child(Some(&page));

    let title = gtk::Label::new(Some("USB Security Center"));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    page.append(&title);

    let subtitle = gtk::Label::new(Some(
        "Monitor USBGuard devices and authorize selected blocked peripherals for the current connection only.",
    ));
    subtitle.add_css_class("dim-label");
    subtitle.set_wrap(true);
    subtitle.set_xalign(0.0);
    page.append(&subtitle);

    let status_card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    status_card.add_css_class("pw-status-card");
    let connection_icon = gtk::Image::from_icon_name("network-transmit-receive-symbolic");
    connection_icon.set_pixel_size(24);
    status_card.append(&connection_icon);

    let status_text = gtk::Box::new(gtk::Orientation::Vertical, 4);
    status_text.set_hexpand(true);
    let status_heading = gtk::Label::new(Some("USBGuard D-Bus connection"));
    status_heading.add_css_class("caption");
    status_heading.add_css_class("dim-label");
    status_heading.set_xalign(0.0);
    status_text.append(&status_heading);
    let connection = gtk::Label::new(Some("Checking…"));
    connection.add_css_class("heading");
    connection.set_xalign(0.0);
    status_text.append(&connection);
    status_card.append(&status_text);

    let read_only = gtk::Label::new(Some("TEMPORARY ONLY"));
    read_only.add_css_class("pw-readonly-pill");
    read_only.set_tooltip_text(Some("Only confirmed temporary approval of eligible external devices is available; no persistent USBGuard policy changes"));
    status_card.append(&read_only);
    page.append(&status_card);

    let stats = gtk::FlowBox::new();
    stats.set_homogeneous(true);
    stats.set_max_children_per_line(3);
    stats.set_min_children_per_line(1);
    stats.set_column_spacing(12);
    stats.set_row_spacing(12);
    stats.set_selection_mode(gtk::SelectionMode::None);
    let (total_tile, total_count) = metric_tile("Total devices", "drive-removable-media-symbolic");
    let (allowed_tile, allowed_count) = metric_tile("Allowed", "emblem-ok-symbolic");
    let (blocked_tile, blocked_count) = metric_tile("Blocked or rejected", "dialog-warning-symbolic");
    stats.append(&total_tile);
    stats.append(&allowed_tile);
    stats.append(&blocked_tile);
    page.append(&stats);

    let device_area = gtk::Box::new(gtk::Orientation::Vertical, 20);
    page.append(&device_area);

    let help = gtk::Label::new(Some(
        "Monitors USBGuard every 3 seconds. Device descriptions are not proof of physical identity.",
    ));
    help.add_css_class("dim-label");
    help.set_wrap(true);
    help.set_xalign(0.0);
    page.append(&help);

    window.set_content(Some(&layout));
    let view = View {
        parent: window.clone(),
        activity: Rc::new(RefCell::new(activity::ActivityHistory::default())),
        connection,
        connection_icon,
        total_count,
        allowed_count,
        blocked_count,
        help,
        device_area,
        refresh_button,
        controllers_expanded: Rc::new(Cell::new(false)),
        last_devices: Rc::new(RefCell::new(None)),
    };

    let button_view = view.clone();
    view.refresh_button.connect_clicked(move |_| refresh(&button_view));

    let activity_view = view.clone();
    activity_button.connect_clicked(move |_| show_activity(&activity_view.parent, &activity_view.activity));

    let security_parent = window.clone();
    security_button.connect_clicked(move |_| show_security(&security_parent));

    let periodic_view = view.clone();
    glib::timeout_add_seconds_local(3, move || {
        refresh(&periodic_view);
        glib::ControlFlow::Continue
    });

    refresh(&view);
    window.present();
}

fn clock_label() -> String {
    glib::DateTime::now_local()
        .ok()
        .and_then(|date| date.format("%H:%M:%S").ok())
        .map(|time| time.to_string())
        .unwrap_or_else(|| "Time unavailable".to_string())
}

fn show_activity(parent: &adw::ApplicationWindow, log: &Rc<RefCell<activity::ActivityHistory>>) {
    let window = gtk::Window::builder()
        .title("PlugWard · Activity")
        .default_width(570)
        .default_height(550)
        .transient_for(parent)
        .build();

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(540);
    scroll.set_child(Some(&clamp));
    let page = gtk::Box::new(gtk::Orientation::Vertical, 14);
    page.set_margin_start(18);
    page.set_margin_end(18);
    page.set_margin_top(24);
    page.set_margin_bottom(22);
    clamp.set_child(Some(&page));

    let heading = gtk::Label::new(Some("Activity observed this session"));
    heading.add_css_class("title-2");
    heading.set_xalign(0.0);
    page.append(&heading);

    let note = gtk::Label::new(Some(
        "This timeline compares sampled USBGuard listings every 3 seconds. It may miss rapid changes and contains no events before PlugWard started. It is not an audit log.",
    ));
    note.set_wrap(true);
    note.set_xalign(0.0);
    note.add_css_class("dim-label");
    page.append(&note);

    let events = log.borrow().entries();
    let group = adw::PreferencesGroup::new();
    group.set_title(&format!("Observed changes ({})", events.len()));
    if events.is_empty() {
        let empty = adw::ActionRow::new();
        empty.set_title("No changes observed yet");
        empty.set_subtitle("Try connecting or disconnecting a trusted, nonessential USB device.");
        group.add(&empty);
    }
    for entry in events {
        let row = adw::ActionRow::new();
        row.set_title(&format!("{} · {}", entry.when, entry.title));
        row.set_subtitle(&entry.description);
        group.add(&row);
    }
    page.append(&group);
    window.set_child(Some(&scroll));
    window.present();
}

fn show_security(parent: &adw::ApplicationWindow) {
    let window = gtk::Window::builder()
        .title("PlugWard · Security status")
        .default_width(580)
        .default_height(430)
        .transient_for(parent)
        .build();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 18);
    root.set_margin_start(24);
    root.set_margin_end(24);
    root.set_margin_top(24);
    root.set_margin_bottom(24);
    let title = gtk::Label::new(Some("USBGuard security status"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    root.append(&title);

    let summary = gtk::Label::new(Some("Checking daemon state and effective default policy…"));
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    root.append(&summary);

    let details = gtk::Label::new(Some(""));
    details.set_xalign(0.0);
    details.set_wrap(true);
    details.set_selectable(true);
    root.append(&details);

    let note = gtk::Label::new(Some(
        "Checks the systemd USBGuard service and daemon-reported ImplicitPolicyTarget. This does not prove that every policy rule is safe or that malicious USB hardware is impossible.",
    ));
    note.add_css_class("dim-label");
    note.set_wrap(true);
    note.set_xalign(0.0);
    root.append(&note);

    let button = gtk::Button::with_label("Recheck");
    button.set_halign(gtk::Align::End);
    root.append(&button);
    let label = summary.clone();
    let details_label = details.clone();
    button.connect_clicked(move |button| {
        run_security_probe(&label, &details_label, button);
    });
    window.set_child(Some(&root));
    run_security_probe(&summary, &details, &button);
    window.present();
}

fn run_security_probe(summary: &gtk::Label, details: &gtk::Label, button: &gtk::Button) {
    if !button.is_sensitive() {
        return;
    }
    button.set_sensitive(false);
    summary.set_text("Checking USBGuard security posture…");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(security::probe());
    });
    let summary = summary.clone();
    let details = details.clone();
    let button = button.clone();
    glib::timeout_add_local(Duration::from_millis(150), move || {
        match receiver.try_recv() {
            Ok(value) => {
                summary.set_text(value.summary());
                details.set_text(&format!(
                    "Daemon service: {}\nEnabled at boot: {}\nImplicit default: {}",
                    format_check(value.daemon_active),
                    format_check(value.boot_enabled),
                    format_check(value.implicit_target),
                ));
                button.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                summary.set_text("Security probe unavailable");
                button.set_sensitive(true);
                glib::ControlFlow::Break
            }
        }
    });
}

fn format_check(value: Result<String, String>) -> String {
    match value {
        Ok(value) => value,
        Err(error) => format!("Unable to verify ({})", usbguard::Device::display_diagnostic(&error)),
    }
}

fn load_styles() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("../data/style.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn metric_tile(title: &str, icon_name: &str) -> (gtk::Box, gtk::Label) {
    let tile = gtk::Box::new(gtk::Orientation::Vertical, 7);
    tile.add_css_class("pw-metric");
    tile.set_hexpand(true);
    tile.set_size_request(145, 105);

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.add_css_class("dim-label");
    top.append(&icon);
    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("dim-label");
    heading.add_css_class("caption");
    heading.set_xalign(0.0);
    top.append(&heading);
    tile.append(&top);

    let number = gtk::Label::new(Some("—"));
    number.add_css_class("title-1");
    number.set_xalign(0.0);
    tile.append(&number);
    (tile, number)
}

fn refresh(view: &View) {
    if !view.refresh_button.is_sensitive() {
        return;
    }
    view.refresh_button.set_sensitive(false);

    // All D-Bus work happens off the UI thread; widgets never cross threads.
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(usbguard::list_devices());
    });

    let view = view.clone();
    glib::timeout_add_local(Duration::from_millis(150), move || {
        match receiver.try_recv() {
            Ok(result) => {
                render(&view, result);
                view.refresh_button.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                render(&view, Err("USBGuard query worker disconnected".into()));
                view.refresh_button.set_sensitive(true);
                glib::ControlFlow::Break
            }
        }
    });
}

/// Turn common D-Bus failures into actionable, least-privilege guidance.
fn connection_help(error: &str) -> (&'static str, &'static str) {
    if error.contains("NameHasNoOwner")
        || error.contains("ServiceUnknown")
        || error.contains("unknown unit")
    {
        (
            "USBGuard D-Bus bridge unavailable",
            "On Fedora: sudo dnf install usbguard-dbus; then sudo systemctl enable --now usbguard-dbus.service. This does not change USBGuard's security policy.",
        )
    } else if error.contains("AccessDenied")
        || error.contains("NotAuthorized")
        || error.contains("Authentication")
    {
        (
            "Permission to list devices denied",
            "Check D-Bus and Polkit permissions for USBGuard device listing. Do not grant broad passwordless USBGuard management access.",
        )
    } else {
        (
            "Unable to read USBGuard",
            "Check that usbguard-dbus.service is running and that D-Bus/Polkit permits device listing. Do not weaken the USBGuard policy to fix PlugWard.",
        )
    }
}

fn render(view: &View, result: Result<Vec<usbguard::Device>, String>) {
    match result {
        Ok(devices) => {
            view.activity.borrow_mut().observe(&devices, &clock_label());
            let allowed = devices.iter().filter(|d| d.target == "allow").count();
            let blocked = devices.iter().filter(|d| d.is_blocked()).count();
            view.connection.set_text("Connected");
            view.connection_icon.set_icon_name(Some("emblem-ok-symbolic"));
            view.total_count.set_text(&devices.len().to_string());
            view.allowed_count.set_text(&allowed.to_string());
            view.blocked_count.set_text(&blocked.to_string());
            view.help.set_text(
                "Updates every 3 seconds · Session activity is sampled, not a complete audit · Names and USB IDs are spoofable.",
            );

            if view.last_devices.borrow().as_ref() != Some(&devices) {
                clear_box(&view.device_area);
                populate_devices(view, &devices);
                *view.last_devices.borrow_mut() = Some(devices);
            }
        }
        Err(error) => {
            view.activity.borrow_mut().reset_observation();
            view.connection.set_text("Connection unavailable");
            view.connection_icon.set_icon_name(Some("dialog-warning-symbolic"));
            view.total_count.set_text("—");
            view.allowed_count.set_text("—");
            view.blocked_count.set_text("—");
            *view.last_devices.borrow_mut() = None;
            clear_box(&view.device_area);

            let (title, guidance) = connection_help(&error);
            view.help.set_text(guidance);
            let group = adw::PreferencesGroup::new();
            group.set_title("Connection error");
            let row = adw::ActionRow::new();
            row.set_title(title);
            row.set_subtitle(&error);
            group.add(&row);
            view.device_area.append(&group);
        }
    }
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn populate_devices(view: &View, devices: &[usbguard::Device]) {
    let blocked: Vec<_> = devices.iter().filter(|device| device.is_blocked()).collect();
    let peripherals: Vec<_> = devices
        .iter()
        .filter(|device| device.kind() != usbguard::DeviceKind::Controller && !device.is_blocked())
        .collect();
    let controllers: Vec<_> = devices
        .iter()
        .filter(|device| device.kind() == usbguard::DeviceKind::Controller && !device.is_blocked())
        .collect();

    if !blocked.is_empty() {
        let group = adw::PreferencesGroup::new();
        group.set_title("Blocked or rejected");
        group.set_description(Some("USBGuard denied access to these devices."));
        for device in blocked {
            group.add(&device_row(device, &view.parent));
        }
        view.device_area.append(&group);
    }

    let devices_group = adw::PreferencesGroup::new();
    devices_group.set_title("Devices");
    devices_group.set_description(Some("Devices authorized by USBGuard, or awaiting a decision."));
    if peripherals.is_empty() {
        let empty = adw::ActionRow::new();
        empty.set_title("No peripheral devices to show");
        empty.set_subtitle("Connect a USB device to see its USBGuard decision.");
        devices_group.add(&empty);
    }
    for device in peripherals {
        devices_group.add(&device_row(device, &view.parent));
    }
    view.device_area.append(&devices_group);

    if !controllers.is_empty() {
        let group = adw::PreferencesGroup::new();
        group.set_title("System components");
        let expander = adw::ExpanderRow::new();
        expander.set_title(&format!("USB root controllers ({})", controllers.len()));
        expander.set_subtitle("Internal USB host controllers · Expand for details");
        expander.set_expanded(view.controllers_expanded.get());
        for controller in controllers {
            expander.add_row(&device_row(controller, &view.parent));
        }
        let expanded_state = view.controllers_expanded.clone();
        expander.connect_expanded_notify(move |row| {
            expanded_state.set(row.is_expanded());
        });
        group.add(&expander);
        view.device_area.append(&group);
    }

    if devices.is_empty() {
        let group = adw::PreferencesGroup::new();
        group.set_title("No USB devices reported");
        view.device_area.append(&group);
    }
}

fn device_row(device: &usbguard::Device, parent: &adw::ApplicationWindow) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(&device.display_name());
    row.set_subtitle(&format!(
        "{} · USB ID {} · Port {} · USBGuard #{}",
        device.connection_label(), device.usb_id, device.port, device.number
    ));

    let icon = gtk::Image::from_icon_name(device.icon_name());
    icon.set_pixel_size(22);
    icon.add_css_class("pw-device-icon");
    row.add_prefix(&icon);

    let status = gtk::Label::new(Some(device.status()));
    status.add_css_class("pw-pill");
    match device.target.as_str() {
        "allow" => status.add_css_class("pw-allowed"),
        "block" | "reject" => status.add_css_class("pw-blocked"),
        _ => status.add_css_class("pw-unknown"),
    }
    row.add_suffix(&status);

    let inspect = gtk::Button::with_label("Details");
    inspect.add_css_class("flat");
    inspect.set_tooltip_text(Some("Inspect reported device attributes without changing USBGuard policy"));
    let snapshot = device.clone();
    let parent = parent.clone();
    inspect.connect_clicked(move |_| details::show(&parent, &snapshot));
    row.add_suffix(&inspect);
    row
}
#[cfg(test)]
mod connection_tests {
    use super::connection_help;

    #[test]
    fn missing_bridge_explains_fedora_setup() {
        let (title, help) = connection_help(
            "GDBus.Error:org.freedesktop.DBus.Error.NameHasNoOwner: activation request failed: unknown unit",
        );
        assert!(title.contains("bridge unavailable"));
        assert!(help.contains("dnf install usbguard-dbus"));
    }

    #[test]
    fn denied_access_does_not_recommend_weakening_policy() {
        let (title, help) = connection_help("org.freedesktop.DBus.Error.AccessDenied");
        assert!(title.contains("denied"));
        assert!(help.contains("Do not grant broad"));
    }
}
