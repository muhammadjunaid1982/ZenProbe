//! UI language: English (default) or Urdu. Urdu strings are returned shaped for display;
//! strings without a translation fall back to English.

use std::sync::atomic::{AtomicBool, Ordering};

static URDU: AtomicBool = AtomicBool::new(false);

pub fn urdu() -> bool {
    URDU.load(Ordering::Relaxed)
}

pub fn set_urdu(on: bool) {
    URDU.store(on, Ordering::Relaxed);
}

pub fn tr(text: &str) -> String {
    if !urdu() {
        return text.to_string();
    }
    match urdu_text(text) {
        Some(u) => crate::rtl::visual(u).into_owned(),
        None => text.to_string(),
    }
}

fn urdu_text(text: &str) -> Option<&'static str> {
    Some(match text {
        // tabs and sections
        "Params" => "پیرامیٹرز",
        "Body" => "باڈی",
        "Query" => "کوئری",
        "Path" => "پاتھ",
        "Headers" => "ہیڈرز",
        "Auth" => "تصدیق",
        "Vars" => "متغیرات",
        "Script" => "اسکرپٹ",
        "Assert" => "جانچ",
        "Tests" => "ٹیسٹ",
        "Docs" => "دستاویزات",
        "Settings" => "ترتیبات",
        "Message" => "پیغام",
        "Messages" => "پیغامات",
        "Metadata" => "میٹا ڈیٹا",
        "Response" => "جواب",
        "Timeline" => "ٹائم لائن",
        "Overview" => "جائزہ",
        "Info" => "معلومات",
        "Pre Request" => "ریکویسٹ سے پہلے",
        "Post Response" => "جواب کے بعد",
        "Variables (JSON)" => "متغیرات (JSON)",
        "Assertions" => "جانچیں",
        "Console" => "کنسول",
        "Name" => "نام",
        "Value" => "قدر",
        "Type" => "قسم",
        "Description" => "تفصیل",
        "Expr" => "اظہار",
        "Operator & value" => "آپریٹر اور قدر",
        // buttons and links
        "Send" => "بھیجیں",
        "Sending…" => "بھیجا جا رہا ہے…",
        "Save" => "محفوظ کریں",
        "Cancel" => "منسوخ کریں",
        "Delete" => "حذف کریں",
        "Rename" => "نام تبدیل کریں",
        "Clone" => "نقل بنائیں",
        "Copy" => "کاپی کریں",
        "Create" => "بنائیں",
        "Import" => "درآمد کریں",
        "Close" => "بند کریں",
        "Clear" => "صاف کریں",
        "Reset" => "دوبارہ ترتیب دیں",
        "Run" => "چلائیں",
        "Stop" => "روکیں",
        "Start" => "شروع کریں",
        "Invoke" => "چلائیں",
        "Running…" => "چل رہا ہے…",
        "Connect" => "جوڑیں",
        "Connecting…" => "جوڑا جا رہا ہے…",
        "Prettify" => "خوبصورت بنائیں",
        "Edit" => "ترمیم",
        "Preview" => "پیش منظر",
        "Bulk Edit" => "اجتماعی ترمیم",
        "Key/Value Edit" => "کلید/قدر ترمیم",
        "Select All" => "سب منتخب کریں",
        "Deselect All" => "سب غیر منتخب کریں",
        "Browse…" => "تلاش کریں…",
        "Add file…" => "فائل شامل کریں…",
        "Don't Save" => "محفوظ نہ کریں",
        "Quit without saving" => "محفوظ کیے بغیر بند کریں",
        // sidebar and menus
        "Collection" => "کلیکشن",
        "Collections" => "کلیکشنز",
        "No collections open" => "کوئی کلیکشن کھلی نہیں",
        "Open collection" => "کلیکشن کھولیں",
        "Create collection" => "کلیکشن بنائیں",
        "Close collection" => "کلیکشن بند کریں",
        "Reload from disk" => "ڈسک سے دوبارہ لوڈ کریں",
        "Collection settings" => "کلیکشن کی ترتیبات",
        "New Request" => "نئی ریکویسٹ",
        "New request" => "نئی ریکویسٹ",
        "New Folder" => "نیا فولڈر",
        "Import cURL" => "cURL درآمد کریں",
        "Load test" => "لوڈ ٹیسٹ",
        "Load test this request" => "اس ریکویسٹ کا لوڈ ٹیسٹ",
        "Load Test" => "لوڈ ٹیسٹ",
        "Runner" => "رنر",
        "Request name" => "ریکویسٹ کا نام",
        "Folder name" => "فولڈر کا نام",
        "Clone Request" => "ریکویسٹ کی نقل",
        "Environments" => "ماحول",
        "Configure" => "ترتیب دیں",
        "Create Environment" => "ماحول بنائیں",
        "Delete Environment" => "ماحول حذف کریں",
        "No environments yet. Create one to get started." => "ابھی کوئی ماحول نہیں۔ شروع کرنے کے لیے ایک بنائیں۔",
        "File format" => "فائل فارمیٹ",
        // tabs context menu
        "Revert changes" => "تبدیلیاں واپس لیں",
        "Clone request" => "ریکویسٹ کی نقل بنائیں",
        "Close others" => "باقی بند کریں",
        "Close to the left" => "بائیں طرف والے بند کریں",
        "Close to the right" => "دائیں طرف والے بند کریں",
        "Close all saved" => "تمام محفوظ شدہ بند کریں",
        "Close (Ctrl+W)" => "بند کریں (Ctrl+W)",
        "Save (Ctrl+S)" => "محفوظ کریں (Ctrl+S)",
        "unsaved tab(s) kept open" => "غیر محفوظ ٹیب کھلے رکھے گئے",
        "Unsaved changes" => "غیر محفوظ تبدیلیاں",
        "You have unsaved changes in this tab." => "اس ٹیب میں غیر محفوظ تبدیلیاں ہیں۔",
        "Some tabs have unsaved changes." => "کچھ ٹیبز میں غیر محفوظ تبدیلیاں ہیں۔",
        // response
        "Pretty" => "خوبصورت",
        "Raw" => "خام",
        "Hex" => "ہیکس",
        "Base64" => "Base64",
        "Filter with JSONPath, e.g. $.data[0].id" => "JSONPath سے فلٹر کریں، مثلاً $.data[0].id",
        "Copy response" => "جواب کاپی کریں",
        "Save response to file" => "جواب فائل میں محفوظ کریں",
        "Clear response" => "جواب صاف کریں",
        "Response copied to clipboard" => "جواب کلپ بورڈ پر کاپی ہو گیا",
        "Response saved to" => "جواب محفوظ ہوا:",
        "Unresolved variables:" => "غیر حل شدہ متغیرات:",
        "These {{variables}} were not defined in the environment, collection, folder, request or runtime and were sent literally." => "یہ {{متغیرات}} کہیں بھی متعین نہیں تھے اس لیے جوں کے توں بھیجے گئے۔",
        "No tests or assertions ran." => "کوئی ٹیسٹ یا جانچ نہیں چلی۔",
        "Request skipped by script (bru.runner.skipRequest)." => "اسکرپٹ نے ریکویسٹ چھوڑ دی (bru.runner.skipRequest)۔",
        "● Connected" => "● منسلک",
        "● Disconnected" => "● منقطع",
        "No body" => "کوئی باڈی نہیں",
        // runner
        "All" => "سب",
        "Passed" => "کامیاب",
        "Failed" => "ناکام",
        "Skipped" => "چھوڑ دیے گئے",
        "Timings" => "اوقات",
        "Delay between requests (ms)" => "ریکویسٹس کے درمیان وقفہ (ms)",
        "Filters" => "فلٹرز",
        "Include tags" => "یہ ٹیگز شامل کریں",
        "Exclude tags" => "یہ ٹیگز خارج کریں",
        "Iterations" => "تکرار",
        // load test
        "Requests per second" => "فی سیکنڈ ریکویسٹس",
        "Response time" => "جواب کا وقت",
        "Status codes" => "اسٹیٹس کوڈز",
        "Errors" => "غلطیاں",
        "no failures" => "کوئی ناکامی نہیں",
        "Configure the test on the left and press Start." => "بائیں طرف ٹیسٹ ترتیب دیں اور شروع کریں دبائیں۔",
        // generate code
        "Generate Code" => "کوڈ بنائیں",
        "Generate code" => "کوڈ بنائیں",
        "Variables are resolved with the selected environment; scripts are not run." => "متغیرات منتخب ماحول سے حل کیے گئے ہیں؛ اسکرپٹس نہیں چلائی گئیں۔",
        // docs
        "No documentation yet. Click Edit to write Markdown docs for this request." => "ابھی کوئی دستاویز نہیں۔ اس ریکویسٹ کے لیے Markdown لکھنے کو ترمیم دبائیں۔",
        "No documentation yet. Click Edit to write Markdown docs." => "ابھی کوئی دستاویز نہیں۔ Markdown لکھنے کو ترمیم دبائیں۔",
        "One `name: value` per line; prefix a line with // to disable it." => "ہر سطر میں ایک `name: value`؛ غیر فعال کرنے کو سطر کے شروع میں // لگائیں۔",
        // preferences, search, status bar
        "Preferences" => "ترجیحات",
        "Language" => "زبان",
        "Zoom" => "زوم",
        "Response layout" => "جواب کی ترتیب",
        "Side by side" => "ساتھ ساتھ",
        "Stacked" => "اوپر نیچے",
        "Keyboard shortcuts" => "کی بورڈ شارٹ کٹس",
        "Search" => "تلاش",
        "Search requests by name, folder or path" => "نام، فولڈر یا پاتھ سے ریکویسٹ تلاش کریں",
        "No requests found" => "کوئی ریکویسٹ نہیں ملی",
        "↑ ↓ to move · Enter to open · Esc to close" => "↑ ↓ منتقل · Enter کھولیں · Esc بند",
        "Toggle response layout (side by side / stacked)" => "جواب کی ترتیب بدلیں",
        "Click to reset (Ctrl+0)" => "ری سیٹ کے لیے کلک کریں (Ctrl+0)",
        "Send request" => "ریکویسٹ بھیجیں",
        "Search requests" => "ریکویسٹس تلاش کریں",
        "Edit environments" => "ماحول میں ترمیم",
        "Close tab" => "ٹیب بند کریں",
        "Close all saved tabs" => "تمام محفوظ ٹیبز بند کریں",
        "Reopen closed tab" => "بند ٹیب دوبارہ کھولیں",
        "Next / previous tab" => "اگلا / پچھلا ٹیب",
        "Go to tab 1–8 / last tab" => "ٹیب 1–8 / آخری ٹیب",
        "Zoom in / out / reset" => "زوم اِن / آؤٹ / ری سیٹ",
        // auth
        "No Auth" => "کوئی تصدیق نہیں",
        "Inherit" => "وراثت",
        "Basic Auth" => "بنیادی تصدیق",
        "Bearer Token" => "بیئرر ٹوکن",
        "Username" => "صارف نام",
        "Password" => "پاس ورڈ",
        "Token" => "ٹوکن",
        "Auth is inherited from the nearest folder or the collection." => "تصدیق قریبی فولڈر یا کلیکشن سے لی جاتی ہے۔",
        "Headers here apply to every request inside." => "یہاں کے ہیڈرز اندر کی ہر ریکویسٹ پر لاگو ہوتے ہیں۔",
        "Location" => "مقام",
        "Format" => "فارمیٹ",
        "Requests" => "ریکویسٹس",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_by_default_and_urdu_when_selected() {
        assert_eq!(tr("Save"), "Save");
        set_urdu(true);
        let u = tr("Save");
        set_urdu(false);
        assert_ne!(u, "Save");
        assert!(u.chars().all(|c| !c.is_ascii_alphabetic()));
        assert_eq!(tr("Unknown string"), "Unknown string");
    }
}
