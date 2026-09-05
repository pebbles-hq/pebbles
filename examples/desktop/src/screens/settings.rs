//! The **Settings** screen — a comprehensive form: company details, currency + tax
//! preferences (saved on submit), and instant-persist toggles. Everything writes
//! through the store to SQLite, so it survives a restart.

use pebbles::prelude::*;

use crate::model::{CURRENCIES, Settings};
use crate::store;

pub fn settings() -> impl IntoWidget {
    component(settings_view)
}

fn settings_view() -> impl IntoWidget {
    let c = theme().colors;
    let s = store::settings(); // subscribes → re-renders when a toggle flips

    let company = create_signal(s.company.clone());
    let email = create_signal(s.email.clone());
    let currency = create_signal(s.currency);
    let tax = create_signal(format!("{:.1}", s.tax_rate));
    let threshold = create_signal(s.low_stock_threshold.to_string());

    let save = move || {
        let cur = store::settings(); // keep the instant-toggle flags as they are
        store::save_settings(Settings {
            company: company.peek().trim().to_string(),
            email: email.peek().trim().to_string(),
            currency: currency.peek(),
            tax_rate: tax.peek().trim().parse().unwrap_or(0.0),
            low_stock_threshold: threshold.peek().trim().parse().unwrap_or(0),
            dark_mode: cur.dark_mode,
            email_notifications: cur.email_notifications,
            weekly_report: cur.weekly_report,
            auto_reorder: cur.auto_reorder,
        });
    };

    // --- Company ------------------------------------------------------------
    let company_card = section(
        "Company",
        "Details shown on invoices and receipts",
        column(children![
            field(text_field().bind(company)).label("Company name"),
            gap_h(12.0),
            field(text_field().bind(email)).label("Contact email"),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget(),
    );

    // --- Preferences (saved on submit) --------------------------------------
    let currency_opts: Vec<String> = CURRENCIES.iter().map(|(sym, code)| format!("{sym}   {code}")).collect();
    let pref_card = section(
        "Preferences",
        "Currency, tax and stock thresholds",
        column(children![
            field(
                select(currency_opts)
                    .value(currency.get())
                    .width(220.0)
                    .on_changed(move |i, _| currency.set(i)),
            )
            .label("Currency"),
            gap_h(12.0),
            row(children![
                Expanded::new(field(text_field().bind(tax).kind(InputKind::Number)).label("Tax rate (%)"),),
                gap_w(12.0),
                Expanded::new(
                    field(text_field().bind(threshold).kind(InputKind::Integer)).label("Low-stock threshold"),
                ),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start),
            gap_h(16.0),
            row(children![button("Save changes").on_pressed(save), spacer()])
                .cross_axis_alignment(CrossAxisAlignment::Center),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget(),
    );

    // --- Alerts (instant persist) -------------------------------------------
    let alerts_card = section(
        "Alerts",
        "These apply immediately",
        column(children![
            switch_list_tile("Dark mode", s.dark_mode)
                .secondary(icon(lucide::MOON).color(c.muted_foreground))
                .on_changed(move || store::set_dark_mode(!store::settings().dark_mode)),
            switch_list_tile("Email notifications", s.email_notifications)
                .secondary(icon(lucide::MAIL).color(c.muted_foreground))
                .on_changed(move || {
                    store::set_flag(|x| x.email_notifications = !x.email_notifications, "notifications")
                }),
            switch_list_tile("Weekly report", s.weekly_report)
                .secondary(icon(lucide::TRENDING_UP).color(c.muted_foreground))
                .on_changed(move || store::set_flag(|x| x.weekly_report = !x.weekly_report, "report")),
            switch_list_tile("Auto-reorder low stock", s.auto_reorder)
                .secondary(icon(lucide::REFRESH_CW).color(c.muted_foreground))
                .on_changed(move || store::set_flag(|x| x.auto_reorder = !x.auto_reorder, "reorder")),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget(),
    );

    scroll_view(
        container().padding(EdgeInsets::all(24.0)).child(
            container().width(720.0).child(
                column(children![company_card, gap_h(18.0), pref_card, gap_h(18.0), alerts_card])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
            ),
        ),
    )
    .drag_scroll(true)
}

fn section(title: &str, subtitle: &str, body: AnyWidget) -> AnyWidget {
    let c = theme().colors;
    container()
        .decoration(
            BoxDecoration::new()
                .color(c.card)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(14.0)),
        )
        .padding(EdgeInsets::all(20.0))
        .child(
            column(children![
                text(title.to_string()).size(15.5).weight(700.0).color(c.foreground),
                gap_h(2.0),
                text(subtitle.to_string()).size(12.5).color(c.muted_foreground),
                gap_h(16.0),
                body,
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        )
        .into_widget()
}
