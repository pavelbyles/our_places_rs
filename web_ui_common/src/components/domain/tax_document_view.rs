//! Statutory tax documents (payment receipts, tax invoices, refund receipts)
//! with the Jamaican GCT figures in JMD. Element IDs and every formatted
//! figure match the legacy component (tests/domain_parity.rs).

use common::tax_document::{
    InvoiceTaxCategory, TaxDocumentResponse, TaxDocumentStatus, TaxDocumentType,
};
use rust_decimal::Decimal;
use topcoat::{
    Result,
    view::{View, class, component, view},
};

use super::price_breakdown::money;

fn percent(rate: Decimal, places: usize) -> String {
    format!("{:.*}%", places, rate * Decimal::ONE_HUNDRED)
}

/// Document title for a document type.
pub fn document_title(kind: TaxDocumentType) -> &'static str {
    match kind {
        TaxDocumentType::PaymentReceipt => "Payment Receipt",
        TaxDocumentType::TaxInvoice => "Statutory Tax Invoice",
        TaxDocumentType::RefundReceipt => "Refund Receipt",
    }
}

/// One formatted line item.
struct LineRow {
    description: String,
    quantity: String,
    unit_price: String,
    category: &'static str,
    tax_rate: String,
    tax_amount: String,
    total: String,
}

/// A printable tax document. Wrap page chrome that should not print in
/// `.no-print`.
#[component]
pub async fn tax_document_view(
    /// The document to show.
    doc: TaxDocumentResponse,
) -> Result<impl View> {
    let cur = doc.currency.clone();
    let title = document_title(doc.document_type);
    let superseded = doc.status == TaxDocumentStatus::Superseded;
    let status = if superseded { "Superseded" } else { "Issued" };
    let issue_date = doc.issue_date.format("%B %d, %Y").to_string();
    let gct_label = format!("Jamaican GCT ({}):", percent(doc.tax_rate, 1));
    let fx = format!("1 {cur} = {:.4} JMD", doc.jmd_exchange_rate);
    let jmd_tax = money("JMD", doc.jmd_tax_amount);
    let jmd_total = money("JMD", doc.jmd_total_amount);
    let acc = money(&cur, doc.accommodation_net);
    let cleaning = (doc.cleaning_fee > Decimal::ZERO).then(|| money(&cur, doc.cleaning_fee));
    let taxable = money(&cur, doc.taxable_amount);
    let tax = money(&cur, doc.tax_amount);
    let platform = (doc.platform_fee > Decimal::ZERO).then(|| money(&cur, doc.platform_fee));
    let deposit = (doc.damage_deposit > Decimal::ZERO).then(|| money(&cur, doc.damage_deposit));
    let total = money(&cur, doc.total_amount);
    let previously_paid = doc.amount_previously_paid.map(|p| money(&cur, p));
    let balance = doc.balance_due.map(|b| {
        if b > Decimal::ZERO {
            ("Balance Due:", money(&cur, b))
        } else if b < Decimal::ZERO {
            ("Refund Due:", money(&cur, -b))
        } else {
            ("Balance Due:", money(&cur, Decimal::ZERO))
        }
    });
    let refund_pct = doc.refund_pct.map(|p| percent(p, 0));
    let retained = doc.retained_amount.map(|r| money(&cur, r));
    let lines: Vec<LineRow> = doc
        .line_items
        .iter()
        .map(|i| LineRow {
            description: i.description.clone(),
            quantity: i.quantity.to_string(),
            unit_price: money(&cur, i.unit_price),
            category: match i.tax_category {
                InvoiceTaxCategory::Gct => "GCT",
                InvoiceTaxCategory::Exempt => "Exempt",
            },
            tax_rate: percent(i.tax_rate, 1),
            tax_amount: money(&cur, i.tax_amount),
            total: money(&cur, i.total_amount),
        })
        .collect();
    let row = "flex justify-between gap-4";
    Ok(view! {
        <article class="tax-document-container space-y-8 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6 text-card-foreground sm:p-10">
            if superseded {
                <div role="note" class="rounded-xl border border-destructive p-4 text-sm">
                    <p class="font-semibold">"SUPERSEDED DOCUMENT"</p>
                    <p class="text-muted-foreground">"This document has been revised. Please refer to the latest revision for accounting purposes."</p>
                </div>
            }
            <header class="flex flex-col justify-between gap-6 sm:flex-row">
                <div>
                    <p class="font-serif text-2xl">"Our Places"</p>
                    <p class="text-sm text-muted-foreground">"Luxury Short-Term Rentals & Villas"</p>
                </div>
                <div class="space-y-1 sm:text-right">
                    <div class="flex items-center gap-2 sm:justify-end">
                        <h1 id="tax-doc-title" class="font-serif text-2xl">(title)</h1>
                        <span id="tax-doc-status" class="rounded-full border border-border px-2 py-0.5 text-xs font-medium">(status)</span>
                    </div>
                    <p id="tax-doc-number" class="font-mono text-sm">(doc.document_number.clone())</p>
                    <p class="text-sm text-muted-foreground">"Issue Date: " <span id="tax-doc-issue-date">(issue_date)</span></p>
                    if doc.revision > 1 {
                        <p class="text-sm text-muted-foreground">"Revision: " (doc.revision.to_string())</p>
                    }
                </div>
            </header>

            <section class="grid gap-6 text-sm sm:grid-cols-2">
                <div class="space-y-1">
                    <p class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">"Supplier / Merchant of Record"</p>
                    <p id="supplier-name" class="font-medium">(doc.supplier_name.clone())</p>
                    <p id="supplier-address">(doc.supplier_address.clone())</p>
                    <p>"TAJ TRN: " <span id="supplier-trn">(doc.supplier_trn.clone())</span></p>
                </div>
                <div class="space-y-1">
                    <p class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">"Recipient / Guest"</p>
                    <p id="recipient-name" class="font-medium">(doc.recipient_name.clone())</p>
                    <p id="recipient-email">(doc.recipient_email.clone())</p>
                    if let Some(company) = doc.recipient_company_name.clone() {
                        <p id="recipient-company">"Company: " (company)</p>
                    }
                    if let Some(trn) = doc.recipient_trn.clone() {
                        <p id="recipient-trn">"Recipient TRN: " <span>(trn)</span></p>
                    }
                    <p>"Booking Reference: " <span>(doc.booking_id.to_string())</span></p>
                </div>
            </section>

            <div class="overflow-x-auto">
                <table class="w-full text-sm">
                    <thead class="text-left text-xs text-muted-foreground uppercase">
                        <tr>
                            <th scope="col">"Description"</th>
                            <th scope="col" class="text-right">"Qty"</th>
                            <th scope="col" class="text-right">"Unit Price"</th>
                            <th scope="col">"Tax Category"</th>
                            <th scope="col" class="text-right">"Tax Rate"</th>
                            <th scope="col" class="text-right">"Tax Amount"</th>
                            <th scope="col" class="text-right">"Total"</th>
                        </tr>
                    </thead>
                    <tbody id="tax-line-items" class="divide-y divide-border">
                        if lines.is_empty() {
                            <tr><td colspan="7" class="py-3 text-muted-foreground">"Accommodation and turnover services itemized below."</td></tr>
                        } else {
                            for line in lines {
                                <tr>
                                    <td class="py-2">(line.description)</td>
                                    <td class="py-2 text-right tabular-nums">(line.quantity)</td>
                                    <td class="py-2 text-right tabular-nums">(line.unit_price)</td>
                                    <td class="py-2">(line.category)</td>
                                    <td class="py-2 text-right tabular-nums">(line.tax_rate)</td>
                                    <td class="py-2 text-right tabular-nums">(line.tax_amount)</td>
                                    <td class="py-2 text-right tabular-nums">(line.total)</td>
                                </tr>
                            }
                        }
                    </tbody>
                </table>
            </div>

            <section class="grid gap-6 sm:grid-cols-2">
                <div class="space-y-2 rounded-xl border border-border p-4 text-sm">
                    <p class="font-semibold">"TAJ Statutory Compliance (Jamaica)"</p>
                    <p class="text-muted-foreground">"Pursuant to the General Consumption Tax Act of Jamaica, tax amounts are converted and reported in statutory Jamaican Dollars (JMD) at the official operational exchange rate on the transaction date."</p>
                    <div class=(row)><span>"Conversion Rate:"</span><span id="statutory-fx-rate" class="tabular-nums">(fx)</span></div>
                    <div class=(row)><span>"Statutory GCT (JMD):"</span><span id="statutory-jmd-tax" class="tabular-nums">(jmd_tax)</span></div>
                    <div class=(row)><span>"Total Statutory Amount (JMD):"</span><span id="statutory-jmd-total" class="tabular-nums">(jmd_total)</span></div>
                </div>
                <dl class="space-y-2 text-sm">
                    <div class=(row)><dt>"Accommodation (Net):"</dt><dd id="tot-acc" class="tabular-nums">(acc)</dd></div>
                    if let Some(cleaning) = cleaning {
                        <div class=(row)><dt>"Departure Cleaning Fee:"</dt><dd id="tot-cleaning" class="tabular-nums">(cleaning)</dd></div>
                    }
                    <div class=(row)><dt>"Taxable Subtotal:"</dt><dd id="tot-taxable" class="tabular-nums">(taxable)</dd></div>
                    <div class=(row)><dt>(gct_label)</dt><dd id="tot-tax" class="tabular-nums">(tax)</dd></div>
                    if let Some(platform) = platform {
                        <div class=(row)><dt>"Platform & Service Fee:"</dt><dd id="tot-platform" class="tabular-nums">(platform)</dd></div>
                    }
                    if let Some(deposit) = deposit {
                        <div class=(row)><dt>"Security Damage Deposit: " <span class="text-muted-foreground">"Held"</span></dt><dd id="tot-deposit" class="tabular-nums">(deposit)</dd></div>
                    }
                    <div class=(class!("border-t border-border pt-2 font-semibold", row))><dt>"Total Amount Paid:"</dt><dd id="tot-total" class="tabular-nums">(total)</dd></div>
                    if let Some(prev) = previously_paid {
                        <div class=(row)><dt>"Amount Previously Paid:"</dt><dd class="tabular-nums">(prev)</dd></div>
                        if let Some((label, amount)) = balance {
                            <div class=(row)><dt>(label)</dt><dd class="tabular-nums">(amount)</dd></div>
                        }
                    }
                    if let Some(pct) = refund_pct {
                        <div class=(row)><dt>"Refund Schedule:"</dt><dd>(pct)</dd></div>
                        if let Some(retained) = retained {
                            <div class=(row)><dt>"Amount Retained by Host:"</dt><dd class="tabular-nums">(retained)</dd></div>
                        }
                    }
                </dl>
            </section>

            <footer class="border-t border-border pt-4 text-xs text-muted-foreground">
                <p>"Our Places Jamaica Limited • Registered in Jamaica"</p>
                <p>"Questions regarding this tax document? Contact concierge@ourplaces.io"</p>
            </footer>
        </article>
    })
}
