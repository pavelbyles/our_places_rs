use common::tax_document::{
    InvoiceTaxCategory, TaxDocumentResponse, TaxDocumentStatus, TaxDocumentType,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use topcoat::{
    Result,
    view::{View, component, view},
};

#[component]
pub async fn tax_document_view(doc: TaxDocumentResponse) -> Result<impl View> {
    let doc_title = match doc.document_type {
        TaxDocumentType::PaymentReceipt => "Payment Receipt",
        TaxDocumentType::TaxInvoice => "Statutory Tax Invoice",
        TaxDocumentType::RefundReceipt => "Refund Receipt",
    };

    let is_superseded = doc.status == TaxDocumentStatus::Superseded;
    let status_str = match doc.status {
        TaxDocumentStatus::Issued => "Issued",
        TaxDocumentStatus::Superseded => "Superseded",
    };

    let curr = doc.currency.clone();
    let issue_date_str = doc.issue_date.format("%B %d, %Y").to_string();
    let gct_rate_pct = format!("{:.1}%", doc.tax_rate * dec!(100));

    let acc_str = format!("{} {:.2}", curr, doc.accommodation_net);
    let cleaning_str = format!("{} {:.2}", curr, doc.cleaning_fee);
    let taxable_str = format!("{} {:.2}", curr, doc.taxable_amount);
    let tax_str = format!("{} {:.2}", curr, doc.tax_amount);
    let platform_str = format!("{} {:.2}", curr, doc.platform_fee);
    let deposit_str = format!("{} {:.2}", curr, doc.damage_deposit);
    let total_str = format!("{} {:.2}", curr, doc.total_amount);

    let jmd_fx_str = format!("1 {} = {:.4} JMD", curr, doc.jmd_exchange_rate);
    let jmd_tax_str = format!("JMD {:.2}", doc.jmd_tax_amount);
    let jmd_total_str = format!("JMD {:.2}", doc.jmd_total_amount);

    Ok(view! {
        <div class="tax-document-container card bg-base-100 border border-base-300 shadow-md rounded-2xl p-6 sm:p-10 space-y-8 font-sans">
            <style>
                r#"
                @media print {
                    .no-print {
                        display: none !important;
                    }
                    body {
                        background: #ffffff !important;
                        color: #000000 !important;
                        margin: 0 !important;
                        padding: 0 !important;
                    }
                    .tax-document-container {
                        border: none !important;
                        box-shadow: none !important;
                        margin: 0 !important;
                        padding: 0 !important;
                        width: 100% !important;
                        max-width: 100% !important;
                    }
                    @page {
                        size: A4;
                        margin: 15mm;
                    }
                }
                "#
            </style>

            if is_superseded {
                <div class="alert alert-warning shadow-sm rounded-xl flex items-center gap-3">
                    <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                    </svg>
                    <div>
                        <div class="font-bold text-sm">"SUPERSEDED DOCUMENT"</div>
                        <div class="text-xs opacity-90">"This document has been revised. Please refer to the latest revision for accounting purposes."</div>
                    </div>
                </div>
            }

            // Header: Branding & Document Identification
            <div class="flex flex-col sm:flex-row justify-between items-start gap-6 border-b border-base-200 pb-6">
                <div>
                    <h2 class="text-2xl font-black tracking-tight text-base-content uppercase font-serif">"Our Places"</h2>
                    <p class="text-xs text-base-content/60 tracking-wider uppercase font-semibold">"Luxury Short-Term Rentals & Villas"</p>
                </div>
                <div class="text-left sm:text-right space-y-1">
                    <div class="flex items-center sm:justify-end gap-2">
                        <h1 class="text-xl font-bold tracking-tight text-base-content" id="tax-doc-title">(doc_title)</h1>
                        if is_superseded {
                            <span class="badge badge-warning font-bold text-xs" id="tax-doc-status">(status_str)</span>
                        } else {
                            <span class="badge badge-success font-bold text-xs" id="tax-doc-status">(status_str)</span>
                        }
                    </div>
                    <div class="text-sm font-mono font-semibold text-primary" id="tax-doc-number">(doc.document_number.clone())</div>
                    <div class="text-xs text-base-content/70">
                        <span>"Issue Date: "</span>
                        <span class="font-medium" id="tax-doc-issue-date">(issue_date_str)</span>
                    </div>
                    if doc.revision > 1 {
                        <div class="text-xs text-base-content/70">
                            <span>"Revision: "</span>
                            <span class="font-medium">(doc.revision.to_string())</span>
                        </div>
                    }
                </div>
            </div>

            // Parties Section: Merchant of Record (Supplier) and Customer (Recipient)
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-8 text-sm">
                <div class="space-y-2 bg-base-200/50 p-4 rounded-xl border border-base-200">
                    <div class="text-xs font-bold uppercase tracking-wider text-base-content/60">"Supplier / Merchant of Record"</div>
                    <div class="font-bold text-base-content" id="supplier-name">(doc.supplier_name.clone())</div>
                    <div class="text-xs text-base-content/80 whitespace-pre-line" id="supplier-address">(doc.supplier_address.clone())</div>
                    <div class="pt-1 text-xs">
                        <span class="font-semibold text-base-content">"TAJ TRN: "</span>
                        <span class="font-mono font-bold text-primary" id="supplier-trn">(doc.supplier_trn.clone())</span>
                    </div>
                </div>

                <div class="space-y-2 bg-base-200/50 p-4 rounded-xl border border-base-200">
                    <div class="text-xs font-bold uppercase tracking-wider text-base-content/60">"Recipient / Guest"</div>
                    <div class="font-bold text-base-content" id="recipient-name">(doc.recipient_name.clone())</div>
                    <div class="text-xs text-base-content/80" id="recipient-email">(doc.recipient_email.clone())</div>
                    if let Some(company) = &doc.recipient_company_name {
                        <div class="text-xs text-base-content/80" id="recipient-company">
                            <span class="font-semibold">"Company: "</span>(company.clone())
                        </div>
                    }
                    if let Some(trn) = &doc.recipient_trn {
                        <div class="pt-1 text-xs" id="recipient-trn">
                            <span class="font-semibold text-base-content">"Recipient TRN: "</span>
                            <span class="font-mono font-bold text-primary">(trn.clone())</span>
                        </div>
                    }
                    <div class="pt-1 text-xs text-base-content/60">
                        <span>"Booking Reference: "</span>
                        <span class="font-mono text-base-content/80">(doc.booking_id.to_string())</span>
                    </div>
                </div>
            </div>

            // Line Items Table
            <div class="overflow-x-auto">
                <table class="table table-sm w-full border border-base-200 rounded-lg">
                    <thead class="bg-base-200 text-xs font-bold uppercase text-base-content/70">
                        <tr>
                            <th class="py-3">"Description"</th>
                            <th class="text-center py-3">"Qty"</th>
                            <th class="text-right py-3">"Unit Price"</th>
                            <th class="text-center py-3">"Tax Category"</th>
                            <th class="text-right py-3">"Tax Rate"</th>
                            <th class="text-right py-3">"Tax Amount"</th>
                            <th class="text-right py-3">"Total"</th>
                        </tr>
                    </thead>
                    <tbody class="text-xs divide-y divide-base-200" id="tax-line-items">
                        if doc.line_items.is_empty() {
                            <tr>
                                <td colspan="7" class="text-center py-4 text-base-content/60 italic">
                                    "Accommodation and turnover services itemized below."
                                </td>
                            </tr>
                        } else {
                            for item in doc.line_items.clone() {
                                let unit_str = format!("{} {:.2}", curr, item.unit_price);
                                let rate_str = format!("{:.1}%", item.tax_rate * dec!(100));
                                let tax_item_str = format!("{} {:.2}", curr, item.tax_amount);
                                let total_item_str = format!("{} {:.2}", curr, item.total_amount);
                                let category_str = match item.tax_category {
                                    InvoiceTaxCategory::Gct => "GCT",
                                    InvoiceTaxCategory::Exempt => "Exempt",
                                };
                                <tr>
                                    <td class="font-medium py-2.5">(item.description)</td>
                                    <td class="text-center py-2.5">(item.quantity.to_string())</td>
                                    <td class="text-right py-2.5">(unit_str)</td>
                                    <td class="text-center py-2.5">
                                        <span class="badge badge-ghost badge-xs font-semibold">(category_str)</span>
                                    </td>
                                    <td class="text-right py-2.5">(rate_str)</td>
                                    <td class="text-right py-2.5">(tax_item_str)</td>
                                    <td class="text-right font-medium py-2.5">(total_item_str)</td>
                                </tr>
                            }
                        }
                    </tbody>
                </table>
            </div>

            // Totals & Statutory dual-currency breakdown
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-8 items-start pt-2">
                // Dual-currency Statutory Compliance Box
                <div class="bg-base-200/40 border border-base-200 p-4 rounded-xl space-y-2 text-xs">
                    <div class="font-bold text-base-content uppercase tracking-wider flex items-center gap-1.5">
                        <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4 text-primary" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
                        </svg>
                        "TAJ Statutory Compliance (Jamaica)"
                    </div>
                    <p class="text-base-content/70">
                        "Pursuant to the General Consumption Tax Act of Jamaica, tax amounts are converted and reported in statutory Jamaican Dollars (JMD) at the official operational exchange rate on the transaction date."
                    </p>
                    <div class="space-y-1 pt-1 border-t border-base-200">
                        <div class="flex justify-between items-center text-base-content/80">
                            <span>"Conversion Rate:"</span>
                            <span class="font-mono font-medium" id="statutory-fx-rate">(jmd_fx_str)</span>
                        </div>
                        <div class="flex justify-between items-center text-base-content/80">
                            <span>"Statutory GCT (JMD):"</span>
                            <span class="font-mono font-medium text-primary" id="statutory-jmd-tax">(jmd_tax_str)</span>
                        </div>
                        <div class="flex justify-between items-center font-bold text-base-content">
                            <span>"Total Statutory Amount (JMD):"</span>
                            <span class="font-mono" id="statutory-jmd-total">(jmd_total_str)</span>
                        </div>
                    </div>
                </div>

                // Right-aligned Financial Totals Summary
                <div class="space-y-2 text-sm bg-base-100 p-2">
                    <div class="flex justify-between items-center text-base-content/80">
                        <span>"Accommodation (Net):"</span>
                        <span class="font-medium" id="tot-acc">(acc_str)</span>
                    </div>
                    if doc.cleaning_fee > Decimal::ZERO {
                        <div class="flex justify-between items-center text-base-content/80">
                            <span>"Departure Cleaning Fee:"</span>
                            <span class="font-medium" id="tot-cleaning">(cleaning_str)</span>
                        </div>
                    }
                    <div class="flex justify-between items-center text-base-content/80 pt-1 border-t border-base-200">
                        <span>"Taxable Subtotal:"</span>
                        <span class="font-medium" id="tot-taxable">(taxable_str)</span>
                    </div>
                    <div class="flex justify-between items-center text-base-content/80">
                        <span>(format!("Jamaican GCT ({}):", gct_rate_pct))</span>
                        <span class="font-medium" id="tot-tax">(tax_str)</span>
                    </div>
                    if doc.platform_fee > Decimal::ZERO {
                        <div class="flex justify-between items-center text-base-content/80">
                            <span>"Platform & Service Fee:"</span>
                            <span class="font-medium" id="tot-platform">(platform_str)</span>
                        </div>
                    }
                    if doc.damage_deposit > Decimal::ZERO {
                        <div class="flex justify-between items-center text-base-content/80">
                            <span class="flex items-center gap-1">
                                "Security Damage Deposit:"
                                <span class="badge badge-ghost badge-xs">"Held"</span>
                            </span>
                            <span class="font-medium" id="tot-deposit">(deposit_str)</span>
                        </div>
                    }

                    <div class="flex justify-between items-center text-base font-bold text-base-content pt-2 border-t-2 border-base-content">
                        <span>"Total Amount Paid:"</span>
                        <span class="text-primary text-lg" id="tot-total">(total_str)</span>
                    </div>

                    // Revised Receipt Reconciliation Block
                    if let Some(prev) = doc.amount_previously_paid {
                        <div class="bg-base-200/60 p-3 rounded-lg space-y-1 text-xs mt-3">
                            <div class="flex justify-between items-center text-base-content/80">
                                <span>"Amount Previously Paid:"</span>
                                <span class="font-medium">(format!("{} {:.2}", curr, prev))</span>
                            </div>
                            if let Some(bal) = doc.balance_due {
                                if bal > Decimal::ZERO {
                                    <div class="flex justify-between items-center font-bold text-error">
                                        <span>"Balance Due:"</span>
                                        <span>(format!("{} {:.2}", curr, bal))</span>
                                    </div>
                                } else if bal < Decimal::ZERO {
                                    <div class="flex justify-between items-center font-bold text-success">
                                        <span>"Refund Due:"</span>
                                        <span>(format!("{} {:.2}", curr, -bal))</span>
                                    </div>
                                } else {
                                    <div class="flex justify-between items-center text-base-content/60">
                                        <span>"Balance Due:"</span>
                                        <span>(format!("{} 0.00", curr))</span>
                                    </div>
                                }
                            }
                        </div>
                    }

                    // Refund Receipt Reconciliation Block
                    if let Some(ref_pct) = doc.refund_pct {
                        <div class="bg-base-200/60 p-3 rounded-lg space-y-1 text-xs mt-3">
                            <div class="flex justify-between items-center text-base-content/80">
                                <span>"Refund Schedule:"</span>
                                <span class="font-medium">(format!("{:.0}%", ref_pct * dec!(100)))</span>
                            </div>
                            if let Some(ret) = doc.retained_amount {
                                <div class="flex justify-between items-center text-base-content/80">
                                    <span>"Amount Retained by Host:"</span>
                                    <span class="font-medium">(format!("{} {:.2}", curr, ret))</span>
                                </div>
                            }
                        </div>
                    }
                </div>
            </div>

            // Footer / Legal Notice
            <div class="pt-6 border-t border-base-200 text-center text-xs text-base-content/50 space-y-1">
                <div>"Our Places Jamaica Limited • Registered in Jamaica"</div>
                <div>"Questions regarding this tax document? Contact concierge@ourplaces.io"</div>
            </div>
        </div>
    })
}
