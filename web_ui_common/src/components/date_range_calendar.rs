//! Server-rendered stay-dates calendar (decision D3). Two native date
//! inputs (always visible: they can be typed into and they are what the form
//! submits) above two months of day buttons with booked and past nights
//! disabled; prev/next re-render the fragment over HTMX. `ui.js` adds range
//! selection and keyboard support and marks the root `data-enhanced`; without
//! it the grid stays hidden.

use chrono::{Datelike, Months, NaiveDate, Weekday};
use common::availability::UnavailableRange;
use topcoat::{
    Result,
    view::{View, component, view},
};

/// Everything the calendar needs to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarProps {
    /// Listing slug, for the fragment URL `/listings/{slug}/calendar`.
    pub listing_slug: String,
    /// First day of the first month shown.
    pub month: NaiveDate,
    /// Number of months shown side by side (2 on desktop).
    pub months: u32,
    /// Today in the listing's time zone; earlier nights are disabled.
    pub today: NaiveDate,
    /// Booked ranges overlapping the months shown.
    pub ranges: Vec<UnavailableRange>,
    /// Selected check-in, if any.
    pub check_in: Option<NaiveDate>,
    /// Selected check-out, if any.
    pub check_out: Option<NaiveDate>,
    /// `id` of the check-in date input (e.g. `reserve-check-in`).
    pub check_in_id: String,
    /// `id` of the check-out date input (e.g. `reserve-check-out`).
    pub check_out_id: String,
}

/// Availability of one night.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayState {
    /// Bookable.
    Available,
    /// Taken by a pending hold or confirmed stay.
    Booked,
    /// Before today.
    Past,
}

/// The first day of `date`'s month.
pub fn first_of_month(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap_or(date)
}

/// Days of the month starting at `first`, padded with `None` to whole
/// Sunday-first weeks.
pub fn month_grid(first: NaiveDate) -> Vec<Option<NaiveDate>> {
    let first = first_of_month(first);
    let lead = first.weekday().num_days_from_sunday() as usize;
    let days: Vec<Option<NaiveDate>> = first
        .iter_days()
        .take_while(|d| d.month() == first.month())
        .map(Some)
        .collect();
    let mut grid = vec![None; lead];
    grid.extend(days);
    let trail = (7 - grid.len() % 7) % 7;
    grid.extend(std::iter::repeat_n(None, trail));
    grid
}

/// Whether the night of `day` is booked (`date_from <= day < date_to`; the
/// checkout day is free for a new check-in).
pub fn is_booked(day: NaiveDate, ranges: &[UnavailableRange]) -> bool {
    ranges.iter().any(|r| r.date_from <= day && day < r.date_to)
}

/// State of the night of `day`.
pub fn day_state(day: NaiveDate, today: NaiveDate, ranges: &[UnavailableRange]) -> DayState {
    if day < today {
        DayState::Past
    } else if is_booked(day, ranges) {
        DayState::Booked
    } else {
        DayState::Available
    }
}

/// Accessible label, e.g. "Tuesday, 12 January 2027, available".
pub fn day_label(day: NaiveDate, state: DayState) -> String {
    let status = match state {
        DayState::Available => "available",
        DayState::Booked => "booked",
        DayState::Past => "unavailable",
    };
    format!("{}, {status}", day.format("%A, %-d %B %Y"))
}

/// URL of the calendar fragment for `month`, keeping the current selection.
pub fn calendar_url(
    slug: &str,
    month: NaiveDate,
    check_in: Option<NaiveDate>,
    check_out: Option<NaiveDate>,
) -> String {
    let mut url = format!("/listings/{slug}/calendar?month={}", month.format("%Y-%m"));
    if let Some(d) = check_in {
        url.push_str(&format!("&check_in={d}"));
    }
    if let Some(d) = check_out {
        url.push_str(&format!("&check_out={d}"));
    }
    url
}

const WEEKDAYS: [Weekday; 7] = [
    Weekday::Sun,
    Weekday::Mon,
    Weekday::Tue,
    Weekday::Wed,
    Weekday::Thu,
    Weekday::Fri,
    Weekday::Sat,
];

/// The stay-dates calendar. Its root is `#booking-dates`, which the quote
/// fragment listens to (`hx-trigger="change from:#booking-dates"`).
#[component]
pub async fn date_range_calendar(
    /// What to render.
    props: CalendarProps,
) -> Result<impl View> {
    let CalendarProps {
        listing_slug,
        month,
        months,
        today,
        ranges,
        check_in,
        check_out,
        check_in_id,
        check_out_id,
    } = props;
    let month = first_of_month(month);
    let prev = month.checked_sub_months(Months::new(1)).unwrap_or(month);
    let next = month.checked_add_months(Months::new(1)).unwrap_or(month);
    let can_go_back = prev >= first_of_month(today);
    let prev_url = calendar_url(&listing_slug, prev, check_in, check_out);
    let next_url = calendar_url(&listing_slug, next, check_in, check_out);
    let shown: Vec<NaiveDate> = (0..months.max(1))
        .filter_map(|i| month.checked_add_months(Months::new(i)))
        .collect();
    let min = today.to_string();
    let check_in_value = check_in.map(|d| d.to_string()).unwrap_or_default();
    let check_out_value = check_out.map(|d| d.to_string()).unwrap_or_default();
    Ok(view! {
        <div
            id="booking-dates"
            data-calendar=""
            data-check-in=(check_in_value.clone())
            data-check-out=(check_out_value.clone())
            class="space-y-4"
        >
            <div data-calendar-fallback="" class="grid grid-cols-2 gap-3">
                <label class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">
                    "Check in"
                    <input type="date" name="check_in" id=(check_in_id) min=(min.clone()) value=(check_in_value) required=(true) class="mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground" />
                </label>
                <label class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">
                    "Check out"
                    <input type="date" name="check_out" id=(check_out_id) min=(min) value=(check_out_value) required=(true) class="mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground" />
                </label>
            </div>
            <div data-calendar-grid="" class="@container space-y-3">
                <div class="flex items-center justify-between">
                    <button
                        type="button"
                        aria-label="Previous month"
                        disabled=(!can_go_back)
                        hx-get=(prev_url)
                        hx-target="#booking-dates"
                        hx-swap="outerHTML"
                        class="size-11 rounded-full border border-border disabled:opacity-40"
                    >"‹"</button>
                    <p aria-live="polite" data-calendar-status="" class="text-sm text-muted-foreground"></p>
                    <button
                        type="button"
                        aria-label="Next month"
                        hx-get=(next_url)
                        hx-target="#booking-dates"
                        hx-swap="outerHTML"
                        class="size-11 rounded-full border border-border"
                    >"›"</button>
                </div>
                <div class="grid gap-6 @2xl:grid-cols-2">
                    for (index, first) in shown.into_iter().enumerate() {
                        let caption = first.format("%B %Y").to_string();
                        let weeks: Vec<Vec<Option<NaiveDate>>> = month_grid(first).chunks(7).map(<[_]>::to_vec).collect();
                        <table role="grid" aria-label=(caption.clone()) class=(if index > 0 { "hidden w-full table-fixed @2xl:table" } else { "w-full table-fixed" })>
                            <caption class="pb-2 font-serif text-lg">(caption)</caption>
                            <thead>
                                <tr>
                                    for wd in WEEKDAYS {
                                        <th scope="col" abbr=(format!("{wd:?}")) class="pb-1 text-xs font-medium text-muted-foreground">(format!("{wd:?}").chars().take(2).collect::<String>())</th>
                                    }
                                </tr>
                            </thead>
                            <tbody>
                                for week in weeks {
                                    <tr>
                                        for cell in week {
                                            if let Some(day) = cell {
                                                let state = day_state(day, today, &ranges);
                                                let disabled = state != DayState::Available;
                                                let selected = Some(day) == check_in || Some(day) == check_out;
                                                let state_attr = match state {
                                                    DayState::Available => "available",
                                                    DayState::Booked => "booked",
                                                    DayState::Past => "past",
                                                };
                                                <td role="gridcell" aria-selected=(if selected { "true" } else { "false" }) class="p-0.5 text-center">
                                                    <button
                                                        type="button"
                                                        data-date=(day.to_string())
                                                        data-state=(state_attr)
                                                        aria-label=(day_label(day, state))
                                                        aria-disabled=(disabled.then_some("true"))
                                                        tabindex="-1"
                                                        class="mx-auto block aspect-square w-full max-w-11 rounded-full text-sm tabular-nums aria-disabled:cursor-not-allowed aria-disabled:text-muted-foreground aria-disabled:line-through data-[selected]:bg-primary data-[selected]:text-primary-foreground data-[in-range]:bg-chip"
                                                    >(day.day())</button>
                                                </td>
                                            } else {
                                                <td role="gridcell" aria-hidden="true"></td>
                                            }
                                        }
                                    </tr>
                                }
                            </tbody>
                        </table>
                    }
                </div>
            </div>
        </div>
    })
}
