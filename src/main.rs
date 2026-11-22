mod usbguard;

use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;

#[derive(Clone)]
struct View {
    connection: gtk::Label,
    summary: gtk::Label,
    help: gtk::Label,
    device_area: gtk::Box,
    refresh_button: gtk::Button,
}

fn main() {
    let application = adw::Application::builder()
        .application_id("io.github.tobiasguta.PlugWard")
        .build();
    application.connect_activate(build_window);
    application.run();
}

fn build_window(application: &adw::Application) {
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("PlugWard")
        .default_width(800)
        .default_height(680)
        .build();

    let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let header = adw::HeaderBar::new();
    let refresh_button = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh USB device list")
        .build();
    header.pack_end(&refresh_button);
    layout.append(&header);

    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    layout.append(&scroll);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 20);
    page.set_margin_start(24);
    page.set_margin_end(24);
    page.set_margin_top(20);
    page.set_margin_bottom(24);
    scroll.set_child(Some(&page));

    let heading = gtk::Label::new(Some("USB Security Center"));
    heading.add_css_class("title-1");
    heading.set_xalign(0.0);
    page.append(&heading);

    let introduction = gtk::Label::new(Some(
        "See how USBGuard handles connected devices. PlugWard is read-only and never changes your security rules.",
    ));
    introduction.set_wrap(true);
    introduction.set_xalign(0.0);
    introduction.add_css_class("dim-label");
    page.append(&introduction);

    let overview = adw::PreferencesGroup::new();
    overview.set_title("Connection");
    let connection_row = adw::ActionRow::new();
    connection_row.set_title("USBGuard");
    let connection = gtk::Label::new(Some("Checking…"));
    connection.add_css_class("dim-label");
    connection_row.add_suffix(&connection);
    overview.add(&connection_row);

    let summary_row = adw::ActionRow::new();
    summary_row.set_title("Device summary");
    let summary = gtk::Label::new(Some("Loading…"));
    summary.add_css_class("dim-label");
    summary_row.add_suffix(&summary);
    overview.add(&summary_row);
    page.append(&overview);

    let device_area = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.append(&device_area);

    let help = gtk::Label::new(Some(
        "PlugWard refreshes every 10 seconds. Your USBGuard daemon continues to enforce its own policy.",
    ));
    help.set_wrap(true);
    help.set_xalign(0.0);
    help.add_css_class("dim-label");
    page.append(&help);

    window.set_content(Some(&layout));
    let view = View {
        connection,
        summary,
        help,
        device_area,
        refresh_button,
    };

    let button_view = view.clone();
    view.refresh_button
        .connect_clicked(move |_| refresh(&button_view));

    let periodic_view = view.clone();
    glib::timeout_add_seconds_local(10, move || {
        refresh(&periodic_view);
        glib::ControlFlow::Continue
    });

    refresh(&view);
    window.present();
}

fn refresh(view: &View) {
    if !view.refresh_button.is_sensitive() {
        return;
    }

    view.refresh_button.set_sensitive(false);
    view.connection.set_text("Checking…");

    // Only data crosses the worker/UI boundary. GTK widgets stay on the
    // main thread; no privileged operation is performed by the worker.
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

fn render(view: &View, result: Result<Vec<usbguard::Device>, String>) {
    while let Some(child) = view.device_area.first_child() {
        view.device_area.remove(&child);
    }

    match result {
        Ok(devices) => {
            let allowed = devices.iter().filter(|device| device.target == "allow").count();
            let blocked = devices
                .iter()
                .filter(|device| device.target == "block" || device.target == "reject")
                .count();

            view.connection.set_text("Connected");
            view.summary.set_text(&format!(
                "{} devices · {allowed} allowed · {blocked} blocked/rejected",
                devices.len()
            ));
            view.help.set_text(
                "Only USBGuard decides whether a device can connect. PlugWard reads its state every 10 seconds; it does not approve devices.",
            );

            let group = adw::PreferencesGroup::new();
            group.set_title("Connected devices");

            if devices.is_empty() {
                let row = adw::ActionRow::new();
                row.set_title("No devices returned");
                row.set_subtitle("USBGuard reported an empty device list.");
                group.add(&row);
            }

            for device in devices {
                let row = adw::ActionRow::new();
                row.set_title(&device.name);
                row.set_subtitle(&format!(
                    "USB ID {} · Port {} · USBGuard device #{}",
                    device.usb_id, device.port, device.number
                ));

                let status = gtk::Label::new(Some(device.status()));
                status.add_css_class("caption");
                match device.target.as_str() {
                    "allow" => status.add_css_class("success"),
                    "block" | "reject" => status.add_css_class("error"),
                    _ => status.add_css_class("dim-label"),
                }
                row.add_suffix(&status);
                group.add(&row);
            }

            view.device_area.append(&group);
        }
        Err(error) => {
            view.connection.set_text("Not available");
            view.summary.set_text("No device data");
            view.help.set_text(
                "Check whether the Fedora usbguard-dbus package is installed and whether D-Bus/Polkit allows device listing. Do not weaken the USBGuard policy to fix the GUI.",
            );
            let group = adw::PreferencesGroup::new();
            group.set_title("Connection error");
            let row = adw::ActionRow::new();
            row.set_title("Unable to read USBGuard");
            row.set_subtitle(&error);
            group.add(&row);
            view.device_area.append(&group);
        }
    }
}
