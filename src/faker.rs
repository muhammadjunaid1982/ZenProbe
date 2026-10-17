//! Bruno's `{{$mock}}` dynamic variables (port of @usebruno/common faker-functions)
//! with small built-in word lists instead of @faker-js/faker.

use std::hash::{BuildHasher, RandomState};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn random_u64() -> u64 {
    RandomState::new().hash_one(SystemTime::now())
}

fn below(n: u64) -> u64 {
    random_u64() % n.max(1)
}

fn pick(list: &[&str]) -> String {
    list[below(list.len() as u64) as usize].to_string()
}

fn chars(set: &[u8], n: usize) -> String {
    (0..n).map(|_| set[below(set.len() as u64) as usize] as char).collect()
}

const ALNUM: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
const HEX: &[u8] = b"0123456789abcdef";
const DIGITS: &[u8] = b"0123456789";
const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";

const FIRST: &[&str] = &["James", "Mary", "Ali", "Fatima", "John", "Aisha", "Robert", "Sara", "Omar", "Emma", "Liam", "Noah", "Olivia", "Zara", "Hassan", "Mia"];
const LAST: &[&str] = &["Smith", "Khan", "Johnson", "Ahmed", "Brown", "Garcia", "Miller", "Davis", "Malik", "Wilson", "Taylor", "Anderson", "Thomas", "Moore"];
const PREFIX: &[&str] = &["Mr.", "Mrs.", "Ms.", "Miss", "Dr."];
const SUFFIX: &[&str] = &["Jr.", "Sr.", "I", "II", "III", "IV", "V", "MD", "DDS", "PhD", "DVM"];
const JOB_AREA: &[&str] = &["Solutions", "Program", "Brand", "Security", "Research", "Marketing", "Directives", "Implementation", "Integration", "Functionality", "Response", "Paradigm", "Tactics", "Identity", "Markets", "Group", "Division", "Applications", "Optimization", "Operations", "Infrastructure", "Intranet", "Communications", "Web", "Branding", "Quality", "Assurance", "Mobility", "Accounts", "Data", "Creative", "Configuration", "Accountability", "Interactions", "Factors", "Usability", "Metrics"];
const JOB_DESC: &[&str] = &["Lead", "Senior", "Direct", "Corporate", "Dynamic", "Future", "Product", "National", "Regional", "District", "Central", "Global", "Customer", "Investor", "International", "Legacy", "Forward", "Internal", "Human", "Chief", "Principal"];
const JOB_TYPE: &[&str] = &["Supervisor", "Associate", "Executive", "Liaison", "Officer", "Manager", "Engineer", "Specialist", "Director", "Coordinator", "Administrator", "Architect", "Analyst", "Designer", "Planner", "Orchestrator", "Technician", "Developer", "Producer", "Consultant", "Assistant", "Facilitator", "Agent", "Representative", "Strategist"];
const CITY: &[&str] = &["Springfield", "Riverside", "Fairview", "Lahore", "Karachi", "Madison", "Georgetown", "Salem", "Franklin", "Clinton", "Arlington", "Ashland", "Dover", "Oxford", "Jackson", "Burlington"];
const STREET: &[&str] = &["Main", "Oak", "Pine", "Maple", "Cedar", "Elm", "Washington", "Lake", "Hill", "Park", "View", "Sunset", "Railroad", "Church", "Mill"];
const STREET_SUFFIX: &[&str] = &["Street", "Avenue", "Road", "Lane", "Drive", "Court", "Way", "Boulevard", "Place"];
const COUNTRY: &[(&str, &str)] = &[("United States", "US"), ("Pakistan", "PK"), ("United Kingdom", "GB"), ("Germany", "DE"), ("France", "FR"), ("Canada", "CA"), ("Japan", "JP"), ("India", "IN"), ("Brazil", "BR"), ("Australia", "AU"), ("Turkey", "TR"), ("United Arab Emirates", "AE"), ("Spain", "ES"), ("Italy", "IT")];
const COLOR: &[&str] = &["red", "green", "blue", "yellow", "purple", "orange", "black", "white", "pink", "cyan", "magenta", "teal", "indigo", "violet", "lime", "olive", "maroon", "silver", "gold", "plum", "orchid", "salmon", "tan", "azure", "lavender", "turquoise", "sky blue", "mint green"];
const ABBR: &[&str] = &["ADP", "AGP", "AI", "API", "ASCII", "CLI", "COM", "CSS", "DNS", "EXE", "FTP", "GB", "HDD", "HEX", "HTTP", "IB", "IP", "JBOD", "JSON", "OCR", "PCI", "PNG", "RAM", "RSS", "SAS", "SCSI", "SDD", "SMS", "SMTP", "SQL", "SSD", "SSL", "TCP", "THX", "TLS", "UDP", "USB", "UTF8", "VGA", "XML", "XSS"];
const NOUN: &[&str] = &["driver", "protocol", "bandwidth", "panel", "microchip", "program", "port", "card", "array", "interface", "system", "sensor", "firewall", "hard drive", "pixel", "alarm", "feed", "monitor", "application", "transmitter", "bus", "circuit", "capacitor", "matrix"];
const VERB: &[&str] = &["back up", "bypass", "hack", "override", "compress", "copy", "navigate", "index", "connect", "generate", "quantify", "calculate", "synthesize", "input", "transmit", "program", "reboot", "parse"];
const INGVERB: &[&str] = &["backing up", "bypassing", "hacking", "overriding", "compressing", "copying", "navigating", "indexing", "connecting", "generating", "quantifying", "calculating", "synthesizing", "transmitting", "programming", "parsing"];
const ADJ: &[&str] = &["auxiliary", "primary", "back-end", "digital", "open-source", "virtual", "cross-platform", "redundant", "online", "haptic", "multi-byte", "bluetooth", "wireless", "1080p", "neural", "optical", "solid state", "mobile"];
const LOREM: &[&str] = &["lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit", "sed", "do", "eiusmod", "tempor", "incididunt", "ut", "labore", "et", "dolore", "magna", "aliqua", "enim", "ad", "minim", "veniam", "quis", "nostrud", "exercitation", "ullamco", "laboris", "nisi", "aliquip", "ex", "ea", "commodo", "consequat"];
const DOMAIN_SUFFIX: &[&str] = &["com", "net", "org", "io", "info", "biz", "name", "dev"];
const EMAIL_DOMAIN: &[&str] = &["gmail.com", "yahoo.com", "hotmail.com", "outlook.com"];
const PROTOCOL: &[&str] = &["http", "https"];
const LOCALE_UA: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_4) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Safari/605.1.15",
    "Mozilla/5.0 (X11; Linux x86_64; rv:125.0) Gecko/20100101 Firefox/125.0",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1",
];
const IMAGE_CATEGORIES: &[(&str, &str)] = &[("randomAbstractImage", "abstract"), ("randomAnimalsImage", "animals"), ("randomBusinessImage", "business"), ("randomCatsImage", "cats"), ("randomCityImage", "city"), ("randomFoodImage", "food"), ("randomNightlifeImage", "nightlife"), ("randomFashionImage", "fashion"), ("randomPeopleImage", "people"), ("randomNatureImage", "nature"), ("randomSportsImage", "sports"), ("randomTransportImage", "transport")];
const TRANSACTION: &[&str] = &["deposit", "withdrawal", "payment", "invoice"];
const CURRENCY: &[(&str, &str, &str)] = &[("USD", "US Dollar", "$"), ("EUR", "Euro", "€"), ("GBP", "Pound Sterling", "£"), ("JPY", "Yen", "¥"), ("PKR", "Pakistan Rupee", "₨"), ("INR", "Indian Rupee", "₹"), ("AED", "UAE Dirham", "د.إ"), ("CAD", "Canadian Dollar", "$")];
const ACCOUNT_NAME: &[&str] = &["Checking Account", "Savings Account", "Money Market Account", "Investment Account", "Credit Card Account", "Personal Loan Account", "Home Loan Account", "Auto Loan Account"];
const COMPANY_SUFFIX: &[&str] = &["Inc", "LLC", "Group", "and Sons", "Ltd"];
const BS_ADJ: &[&str] = &["clicks-and-mortar", "value-added", "vertical", "proactive", "robust", "revolutionary", "scalable", "leading-edge", "innovative", "intuitive", "strategic", "e-business", "mission-critical", "sticky", "one-to-one", "24/7", "end-to-end", "global", "B2B", "B2C", "granular", "frictionless", "virtual", "viral", "dynamic", "real-time", "seamless"];
const BS_BUZZ: &[&str] = &["implement", "utilize", "integrate", "streamline", "optimize", "evolve", "transform", "embrace", "enable", "orchestrate", "leverage", "reinvent", "aggregate", "architect", "enhance", "incentivize", "morph", "empower", "envisioneer", "monetize", "harness", "facilitate", "seize", "disintermediate", "synergize", "strategize", "deploy", "brand", "grow", "target", "syndicate", "synthesize", "deliver", "mesh", "incubate", "engage", "maximize", "benchmark", "expedite", "reintermediate", "whiteboard", "visualize", "repurpose", "innovate", "scale", "unleash", "drive", "extend", "engineer", "revolutionize", "generate", "exploit", "transition", "e-enable", "iterate", "cultivate", "matrix", "productize", "redefine", "recontextualize"];
const BS_NOUN: &[&str] = &["synergies", "web-readiness", "paradigms", "markets", "partnerships", "infrastructures", "platforms", "initiatives", "channels", "eyeballs", "communities", "ROI", "solutions", "e-tailers", "e-services", "action-items", "portals", "niches", "technologies", "content", "vortals", "supply-chains", "convergence", "relationships", "architectures", "interfaces", "e-markets", "e-commerce", "systems", "bandwidth", "infomediaries", "models", "mindshare", "deliverables", "users", "schemas", "networks", "applications", "metrics", "e-business", "functionalities", "experiences", "web services", "methodologies"];
const CP_ADJ: &[&str] = &["Adaptive", "Advanced", "Ameliorated", "Assimilated", "Automated", "Balanced", "Business-focused", "Centralized", "Cloned", "Compatible", "Configurable", "Cross-group", "Customer-focused", "Customizable", "Decentralized", "De-engineered", "Devolved", "Digitized", "Distributed", "Diverse", "Down-sized", "Enhanced", "Enterprise-wide", "Ergonomic", "Exclusive", "Expanded", "Extended", "Face to face", "Focused", "Front-line", "Fully-configurable", "Fundamental", "Future-proofed", "Grass-roots", "Horizontal", "Implemented", "Innovative", "Integrated", "Intuitive", "Inverse", "Managed", "Mandatory", "Monitored", "Multi-channelled", "Multi-lateral", "Multi-layered", "Networked", "Object-based", "Open-architected", "Open-source", "Operative", "Optimized", "Optional", "Organic", "Organized", "Persevering", "Persistent", "Phased", "Polarised", "Pre-emptive", "Proactive", "Profit-focused", "Profound", "Programmable", "Progressive", "Public-key", "Quality-focused", "Reactive", "Realigned", "Re-contextualized", "Re-engineered", "Reduced", "Reverse-engineered", "Right-sized", "Robust", "Seamless", "Secured", "Self-enabling", "Sharable", "Smart", "Streamlined", "Switchable", "Synchronised", "Synergistic", "Synergized", "Team-oriented", "Total", "Triple-buffered", "Universal", "Up-sized", "Upgradable", "User-centric", "User-friendly", "Versatile", "Virtual", "Visionary", "Vision-oriented"];
const CP_DESC: &[&str] = &["24 hour", "24/7", "3rd generation", "4th generation", "5th generation", "6th generation", "actuating", "analyzing", "asymmetric", "asynchronous", "attitude-oriented", "background", "bandwidth-monitored", "bi-directional", "bifurcated", "bottom-line", "clear-thinking", "client-driven", "client-server", "coherent", "cohesive", "composite", "context-sensitive", "contextually-based", "content-based", "dedicated", "demand-driven", "didactic", "directional", "discrete", "disintermediate", "dynamic", "eco-centric", "empowering", "encompassing", "even-keeled", "executive", "explicit", "exuding", "fault-tolerant", "foreground", "fresh-thinking", "full-range", "global", "grid-enabled", "heuristic", "high-level", "holistic", "homogeneous", "human-resource", "hybrid", "impactful", "incremental", "intangible", "interactive", "intermediate", "leading edge", "local", "logistical", "maximized", "methodical", "mission-critical", "mobile", "modular", "motivating", "multimedia", "multi-state", "multi-tasking", "national", "needs-based", "neutral", "next generation", "non-volatile", "object-oriented", "optimal", "optimizing", "radical", "real-time", "reciprocal", "regional", "responsive", "scalable", "secondary", "solution-oriented", "stable", "static", "systematic", "systemic", "system-worthy", "tangible", "tertiary", "transitional", "uniform", "upward-trending", "user-facing", "value-added", "web-enabled", "well-modulated", "zero administration", "zero defect", "zero tolerance"];
const CP_NOUN: &[&str] = &["ability", "access", "adapter", "algorithm", "alliance", "analyzer", "application", "approach", "architecture", "archive", "artificial intelligence", "array", "attitude", "benchmark", "budgetary management", "capability", "capacity", "challenge", "circuit", "collaboration", "complexity", "concept", "conglomeration", "contingency", "core", "customer loyalty", "database", "data-warehouse", "definition", "emulation", "encoding", "encryption", "extranet", "firmware", "flexibility", "focus group", "forecast", "frame", "framework", "function", "functionalities", "Graphic Interface", "groupware", "Graphical User Interface", "hardware", "help-desk", "hierarchy", "hub", "implementation", "info-mediaries", "infrastructure", "initiative", "installation", "instruction set", "interface", "internet solution", "intranet", "knowledge user", "knowledge base", "local area network", "leverage", "matrices", "matrix", "methodology", "middleware", "migration", "model", "moderator", "monitoring", "moratorium", "neural-net", "open architecture", "open system", "orchestration", "paradigm", "parallelism", "policy", "portal", "pricing structure", "process improvement", "product", "productivity", "project", "projection", "protocol", "secured line", "service-desk", "software", "solution", "standardization", "strategy", "structure", "success", "superstructure", "support", "synergy", "system engine", "task-force", "throughput", "time-frame", "toolset", "utilisation", "website", "workforce"];
const DB_COLUMN: &[&str] = &["id", "title", "name", "email", "phone", "token", "group", "category", "password", "comment", "avatar", "status", "createdAt", "updatedAt"];
const DB_TYPE: &[&str] = &["int", "varchar", "text", "date", "datetime", "tinyint", "time", "timestamp", "smallint", "mediumint", "bigint", "decimal", "float", "double", "real", "bit", "boolean", "serial", "blob", "binary", "enum", "set", "geometry", "point"];
const DB_COLLATION: &[&str] = &["utf8_unicode_ci", "utf8_general_ci", "utf8_bin", "ascii_bin", "ascii_general_ci", "cp1250_bin", "cp1250_general_ci"];
const DB_ENGINE: &[&str] = &["InnoDB", "MyISAM", "MEMORY", "CSV", "BLACKHOLE", "ARCHIVE"];
const WEEKDAY: &[&str] = &["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const MONTH: &[&str] = &["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const FILE_EXT: &[(&str, &str, &str)] = &[("pdf", "application", "application/pdf"), ("json", "application", "application/json"), ("zip", "application", "application/zip"), ("png", "image", "image/png"), ("jpeg", "image", "image/jpeg"), ("gif", "image", "image/gif"), ("mp3", "audio", "audio/mpeg"), ("wav", "audio", "audio/wav"), ("mp4", "video", "video/mp4"), ("html", "text", "text/html"), ("txt", "text", "text/plain"), ("csv", "text", "text/csv")];
const DIRS: &[&str] = &["/usr/bin", "/etc", "/var/log", "/home/user", "/opt/app", "/tmp", "/usr/local/share", "/srv"];
const PRODUCT: &[&str] = &["Chair", "Car", "Computer", "Keyboard", "Mouse", "Bike", "Ball", "Gloves", "Pants", "Shirt", "Table", "Shoes", "Hat", "Towels", "Soap", "Tuna", "Chicken", "Fish", "Cheese", "Bacon", "Pizza", "Salad", "Sausages", "Chips"];
const PRODUCT_ADJ: &[&str] = &["Small", "Ergonomic", "Electronic", "Rustic", "Intelligent", "Gorgeous", "Incredible", "Elegant", "Fantastic", "Practical", "Modern", "Recycled", "Sleek", "Bespoke", "Awesome", "Generic", "Handcrafted", "Handmade", "Oriental", "Licensed", "Luxurious", "Refined", "Unbranded", "Tasty"];
const MATERIAL: &[&str] = &["Steel", "Bronze", "Wooden", "Concrete", "Plastic", "Cotton", "Granite", "Rubber", "Metal", "Soft", "Fresh", "Frozen"];
const DEPARTMENT: &[&str] = &["Books", "Movies", "Music", "Games", "Electronics", "Computers", "Home", "Garden", "Tools", "Grocery", "Health", "Beauty", "Toys", "Kids", "Baby", "Clothing", "Shoes", "Jewelery", "Sports", "Outdoors", "Automotive", "Industrial"];

/// Every `{{$name}}` Bruno supports, in Bruno's order.
#[cfg(test)]
const NAMES: &[&str] = &[
    "guid", "timestamp", "isoTimestamp", "randomUUID", "randomNanoId", "randomAlphaNumeric", "randomBoolean", "randomInt", "randomColor", "randomHexColor", "randomAbbreviation", "randomIP", "randomIPV4", "randomIPV6", "randomMACAddress", "randomPassword", "randomLocale", "randomUserAgent", "randomProtocol", "randomSemver", "randomFirstName", "randomLastName", "randomFullName", "randomNamePrefix", "randomNameSuffix", "randomJobArea", "randomJobDescriptor", "randomJobTitle", "randomJobType", "randomPhoneNumber", "randomPhoneNumberExt", "randomCity", "randomStreetName", "randomStreetAddress", "randomCountry", "randomCountryCode", "randomLatitude", "randomLongitude", "randomAvatarImage", "randomImageUrl", "randomAbstractImage", "randomAnimalsImage", "randomBusinessImage", "randomCatsImage", "randomCityImage", "randomFoodImage", "randomNightlifeImage", "randomFashionImage", "randomPeopleImage", "randomNatureImage", "randomSportsImage", "randomTransportImage", "randomImageDataUri", "randomBankAccount", "randomBankAccountName", "randomCreditCardMask", "randomBankAccountBic", "randomBankAccountIban", "randomTransactionType", "randomCurrencyCode", "randomCurrencyName", "randomCurrencySymbol", "randomBitcoin", "randomCompanyName", "randomCompanySuffix", "randomBs", "randomBsAdjective", "randomBsBuzz", "randomBsNoun", "randomCatchPhrase", "randomCatchPhraseAdjective", "randomCatchPhraseDescriptor", "randomCatchPhraseNoun", "randomDatabaseColumn", "randomDatabaseType", "randomDatabaseCollation", "randomDatabaseEngine", "randomDateFuture", "randomDatePast", "randomDateRecent", "randomWeekday", "randomMonth", "randomDomainName", "randomDomainSuffix", "randomDomainWord", "randomEmail", "randomExampleEmail", "randomUserName", "randomUrl", "randomFileName", "randomFileType", "randomFileExt", "randomCommonFileName", "randomCommonFileType", "randomCommonFileExt", "randomFilePath", "randomDirectoryPath", "randomMimeType", "randomPrice", "randomProduct", "randomProductAdjective", "randomProductMaterial", "randomProductName", "randomDepartment", "randomNoun", "randomVerb", "randomIngverb", "randomAdjective", "randomWord", "randomWords", "randomPhrase", "randomLoremWord", "randomLoremWords", "randomLoremSentence", "randomLoremSentences", "randomLoremParagraph", "randomLoremParagraphs", "randomLoremText", "randomLoremSlug", "randomLoremLines",
];

pub fn uuid() -> String {
    let (a, b) = (random_u64(), random_u64());
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        a >> 32, (a >> 16) & 0xffff, a & 0xfff, (b >> 48) & 0x3fff | 0x8000, b & 0xffff_ffff_ffff
    )
}

/// ISO-8601 UTC timestamp (`2026-10-17T12:00:00.000Z`) for milliseconds since the epoch.
pub fn iso(ms: i64) -> String {
    let (secs, ms) = (ms.div_euclid(1000), ms.rem_euclid(1000));
    let (days, sod) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil_from_days (H. Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{ms:03}Z", sod / 3600, sod % 3600 / 60, sod % 60)
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn words(n: usize) -> String {
    (0..n).map(|_| pick(LOREM)).collect::<Vec<_>>().join(" ")
}

fn sentence() -> String {
    let mut s = words(4 + below(6) as usize);
    s[..1].make_ascii_uppercase();
    s + "."
}

fn sentences(n: usize, sep: &str) -> String {
    (0..n).map(|_| sentence()).collect::<Vec<_>>().join(sep)
}

fn paragraph() -> String {
    sentences(3, " ")
}

fn first() -> String {
    pick(FIRST)
}

fn last() -> String {
    pick(LAST)
}

fn domain_word() -> String {
    format!("{}-{}", pick(ADJ).replace(' ', "-"), pick(NOUN).replace(' ', "-")).to_lowercase()
}

fn domain() -> String {
    format!("{}.{}", domain_word(), pick(DOMAIN_SUFFIX))
}

fn username() -> String {
    format!("{}.{}{}", first(), last(), below(100))
}

fn phone() -> String {
    format!("({}) {}-{}", 200 + below(800), 200 + below(800), chars(DIGITS, 4))
}

fn iban() -> String {
    format!("GB{}{}{}", chars(DIGITS, 2), chars(UPPER, 4), chars(DIGITS, 14))
}

fn company() -> String {
    match below(3) {
        0 => format!("{} {}", last(), pick(COMPANY_SUFFIX)),
        1 => format!("{}-{}", last(), last()),
        _ => format!("{}, {} and {}", last(), last(), last()),
    }
}

fn file_ext() -> (&'static str, &'static str, &'static str) {
    FILE_EXT[below(FILE_EXT.len() as u64) as usize]
}

fn file_name() -> String {
    format!("{}.{}", words(2).replace(' ', "_"), file_ext().0)
}

fn price() -> String {
    format!("{}.{:02}", 1 + below(999), below(100))
}

fn date_offset(min_days: i64, max_days: i64) -> String {
    let span = (max_days - min_days).max(1) as u64 * 86_400_000;
    iso(now_ms() + min_days * 86_400_000 + below(span) as i64)
}

/// Value of `{{$name}}`, or None for names Bruno doesn't define.
pub fn mock(name: &str) -> Option<String> {
    if let Some((_, cat)) = IMAGE_CATEGORIES.iter().find(|(n, _)| *n == name) {
        return Some(format!("https://loremflickr.com/640/480/{cat}?lock={}", below(1_000_000)));
    }
    Some(match name {
        "guid" | "randomUUID" => uuid(),
        "timestamp" => (now_ms() / 1000).to_string(),
        "isoTimestamp" => iso(now_ms()),
        "randomNanoId" => chars(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-", 21),
        "randomAlphaNumeric" => chars(ALNUM, 1),
        "randomBoolean" => (below(2) == 1).to_string(),
        "randomInt" => below(1001).to_string(),
        "randomColor" => pick(COLOR),
        "randomHexColor" => format!("#{}", chars(HEX, 6)),
        "randomAbbreviation" => pick(ABBR),
        "randomIP" | "randomIPV4" => format!("{}.{}.{}.{}", below(256), below(256), below(256), below(256)),
        "randomIPV6" => (0..8).map(|_| chars(HEX, 4)).collect::<Vec<_>>().join(":"),
        "randomMACAddress" => (0..6).map(|_| chars(HEX, 2)).collect::<Vec<_>>().join(":"),
        "randomPassword" => chars(b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789_-", 15),
        "randomLocale" | "randomCountryCode" => COUNTRY[below(COUNTRY.len() as u64) as usize].1.to_string(),
        "randomCountry" => COUNTRY[below(COUNTRY.len() as u64) as usize].0.to_string(),
        "randomUserAgent" => pick(LOCALE_UA),
        "randomProtocol" => pick(PROTOCOL),
        "randomSemver" => format!("{}.{}.{}", below(10), below(10), below(10)),
        "randomFirstName" => first(),
        "randomLastName" => last(),
        "randomFullName" => format!("{} {}", first(), last()),
        "randomNamePrefix" => pick(PREFIX),
        "randomNameSuffix" => pick(SUFFIX),
        "randomJobArea" => pick(JOB_AREA),
        "randomJobDescriptor" => pick(JOB_DESC),
        "randomJobTitle" => format!("{} {} {}", pick(JOB_DESC), pick(JOB_AREA), pick(JOB_TYPE)),
        "randomJobType" => pick(JOB_TYPE),
        "randomPhoneNumber" => phone(),
        "randomPhoneNumberExt" => format!("{} x{}", phone(), chars(DIGITS, 3)),
        "randomCity" => pick(CITY),
        "randomStreetName" => format!("{} {}", pick(STREET), pick(STREET_SUFFIX)),
        "randomStreetAddress" => format!("{} {} {}", 1 + below(9999), pick(STREET), pick(STREET_SUFFIX)),
        "randomLatitude" => format!("{:.4}", below(1_800_001) as f64 / 10_000.0 - 90.0),
        "randomLongitude" => format!("{:.4}", below(3_600_001) as f64 / 10_000.0 - 180.0),
        "randomAvatarImage" => format!("https://avatars.githubusercontent.com/u/{}", below(100_000_000)),
        "randomImageUrl" => format!("https://picsum.photos/seed/{}/640/480", chars(ALNUM, 8)),
        "randomImageDataUri" => {
            let svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"640\" height=\"480\"><rect width=\"100%\" height=\"100%\" fill=\"#{}\"/></svg>", chars(HEX, 6));
            format!("data:image/svg+xml;charset=UTF-8,{}", svg.replace('<', "%3C").replace('>', "%3E").replace('"', "%22").replace('#', "%23").replace(' ', "%20"))
        }
        "randomBankAccount" => chars(DIGITS, 8),
        "randomBankAccountName" => pick(ACCOUNT_NAME),
        "randomCreditCardMask" => {
            let i = iban();
            let n = i.len();
            i.char_indices().map(|(k, c)| if k >= 4 && k + 2 < n { '*' } else { c }).collect()
        }
        "randomBankAccountBic" => format!("{}{}{}", chars(UPPER, 4), chars(UPPER, 2), chars(UPPER, 2)),
        "randomBankAccountIban" => iban(),
        "randomTransactionType" => pick(TRANSACTION),
        "randomCurrencyCode" => CURRENCY[below(CURRENCY.len() as u64) as usize].0.to_string(),
        "randomCurrencyName" => CURRENCY[below(CURRENCY.len() as u64) as usize].1.to_string(),
        "randomCurrencySymbol" => CURRENCY[below(CURRENCY.len() as u64) as usize].2.to_string(),
        "randomBitcoin" => format!("1{}", chars(b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz", 33)),
        "randomCompanyName" | "randomCompanySuffix" => company(),
        "randomBs" => format!("{} {} {}", pick(BS_BUZZ), pick(BS_ADJ), pick(BS_NOUN)),
        "randomBsAdjective" => pick(BS_ADJ),
        "randomBsBuzz" => pick(BS_BUZZ),
        "randomBsNoun" => pick(BS_NOUN),
        "randomCatchPhrase" => format!("{} {} {}", pick(CP_ADJ), pick(CP_DESC), pick(CP_NOUN)),
        "randomCatchPhraseAdjective" => pick(CP_ADJ),
        "randomCatchPhraseDescriptor" => pick(CP_DESC),
        "randomCatchPhraseNoun" => pick(CP_NOUN),
        "randomDatabaseColumn" => pick(DB_COLUMN),
        "randomDatabaseType" => pick(DB_TYPE),
        "randomDatabaseCollation" => pick(DB_COLLATION),
        "randomDatabaseEngine" => pick(DB_ENGINE),
        "randomDateFuture" => date_offset(1, 365),
        "randomDatePast" => date_offset(-365, -1),
        "randomDateRecent" => date_offset(-1, 0),
        "randomWeekday" => pick(WEEKDAY),
        "randomMonth" => pick(MONTH),
        "randomDomainName" => domain(),
        "randomDomainSuffix" => pick(DOMAIN_SUFFIX),
        "randomDomainWord" => domain_word(),
        "randomEmail" => format!("{}@{}", username(), pick(EMAIL_DOMAIN)),
        "randomExampleEmail" => format!("{}@example.{}", username(), pick(&["com", "net", "org"])),
        "randomUserName" => username(),
        "randomUrl" => format!("https://{}/", domain()),
        "randomFileName" | "randomCommonFileName" => file_name(),
        "randomFileType" | "randomCommonFileType" => file_ext().1.to_string(),
        "randomFileExt" | "randomCommonFileExt" => file_ext().0.to_string(),
        "randomFilePath" => format!("{}/{}", pick(DIRS), file_name()),
        "randomDirectoryPath" => pick(DIRS),
        "randomMimeType" => file_ext().2.to_string(),
        "randomPrice" => price(),
        "randomProduct" => pick(PRODUCT),
        "randomProductAdjective" => pick(PRODUCT_ADJ),
        "randomProductMaterial" => pick(MATERIAL),
        "randomProductName" => format!("{} {} {}", pick(PRODUCT_ADJ), pick(MATERIAL), pick(PRODUCT)),
        "randomDepartment" => pick(DEPARTMENT),
        "randomNoun" | "randomWord" => pick(NOUN),
        "randomVerb" => pick(VERB),
        "randomIngverb" => pick(INGVERB),
        "randomAdjective" => pick(ADJ),
        "randomWords" | "randomLoremWords" => words(3),
        "randomPhrase" => format!("If we {} the {}, we can get to the {} {} through the {} {}!", pick(VERB), pick(NOUN), pick(ADJ), pick(NOUN), pick(ADJ), pick(NOUN)),
        "randomLoremWord" => pick(LOREM),
        "randomLoremSentence" => sentence(),
        "randomLoremSentences" => sentences(2 + below(5) as usize, " "),
        "randomLoremParagraph" => paragraph(),
        "randomLoremParagraphs" => (0..3).map(|_| paragraph()).collect::<Vec<_>>().join("\n"),
        "randomLoremText" => paragraph(),
        "randomLoremSlug" => words(3).replace(' ', "-"),
        "randomLoremLines" => sentences(1 + below(5) as usize, "\n"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_generates() {
        for n in NAMES {
            assert!(mock(n).is_some_and(|v| !v.is_empty()), "{n}");
        }
        assert_eq!(NAMES.len(), 120);
        assert!(mock("nope").is_none());
    }

    #[test]
    fn iso_dates() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso(1_709_208_000_123), "2024-02-29T12:00:00.123Z");
    }
}
