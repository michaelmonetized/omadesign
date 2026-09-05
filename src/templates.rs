//! An original, editable design bank. No stock assets, raster placeholders or network calls.

use crate::color::Rgba;
use crate::document::{Artboard, Document, Fill, Layer, Shape, Stroke, Style};
use crate::geom::{Geom, Pt, TypeRun};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Orbit,
    Grid,
    Botanical,
    Wave,
    Letters,
    Architecture,
    Ribbon,
    Ticket,
    Horizon,
    Signal,
    Objects,
    Mosaic,
    Frame,
}

#[derive(Clone, Copy, Debug)]
pub struct Template {
    pub id: &'static str,
    pub week: u8,
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub kicker: &'static str,
    pub title: &'static str,
    pub body: &'static str,
    pub footer: &'static str,
    /// Paper, ink, accent, secondary; packed RGB.
    pub palette: [u32; 4],
    pub family: Layout,
    pub variant: u8,
}

macro_rules! entry {
    ($week:literal, $family:ident, $variant:literal, $id:literal, $name:literal, $category:literal, $description:literal, $kicker:literal, $title:literal, $body:literal, $footer:literal, $paper:literal, $ink:literal, $accent:literal, $secondary:literal) => {
        Template {
            week: $week,
            family: Layout::$family,
            variant: $variant,
            id: $id,
            name: $name,
            category: $category,
            description: $description,
            kicker: $kicker,
            title: $title,
            body: $body,
            footer: $footer,
            palette: [$paper, $ink, $accent, $secondary],
        }
    };
}

pub const CATALOG: &[Template] = &[
    entry!(
        1,
        Orbit,
        0,
        "after-hours",
        "After Hours",
        "Events",
        "Circular rhythms for a late-night listening session.",
        "LISTENING ROOM / VOL. 01",
        "AFTER\nHOURS",
        "Good records. Low lights. Open ears.",
        "FRIDAY / 20:00 / ALL WELCOME",
        0x161923,
        0xF6F0E2,
        0xEBAF62,
        0xAA8DD8
    ),
    entry!(
        2,
        Orbit,
        1,
        "solar-social",
        "Solar Social",
        "Community",
        "A sunny invitation built from an orbiting constellation.",
        "GOOD COMPANY, OUTSIDE",
        "SOLAR\nSOCIAL",
        "Bring a blanket. Meet your neighbours.",
        "SUNDAY / THE COMMON / 12:00",
        0xFFF5DB,
        0x24392D,
        0xEF6E45,
        0xE5B736
    ),
    entry!(
        3,
        Orbit,
        2,
        "connected",
        "Connected",
        "Brand",
        "An optimistic identity for people making useful connections.",
        "INDEPENDENT IDEAS, TOGETHER",
        "CONNECTED",
        "Small introductions. New possibilities.",
        "A COMMUNITY FOR CURIOUS PEOPLE",
        0xE7F0EB,
        0x1E4140,
        0x326DCE,
        0xE8856A
    ),
    entry!(
        4,
        Orbit,
        3,
        "night-school",
        "Night School",
        "Education",
        "A nocturnal workshop identity with a radiating diagram.",
        "LEARN SOMETHING AFTER DARK",
        "NIGHT\nSCHOOL",
        "A little curiosity goes a long way.",
        "THURSDAYS / STUDIO 04 / 18:30",
        0x202444,
        0xF6ECCD,
        0xB7C674,
        0xE794AD
    ),
    entry!(
        5,
        Grid,
        0,
        "block-party",
        "Block Party",
        "Community",
        "Playful city blocks for a neighbourhood get-together.",
        "ONE STREET. MANY STORIES.",
        "BLOCK\nPARTY",
        "Food, music and familiar faces.",
        "SATURDAY / YOUR NEIGHBOURHOOD",
        0xF6EBDD,
        0x263B37,
        0xE87752,
        0xC1C554
    ),
    entry!(
        6,
        Grid,
        1,
        "better-systems",
        "Better Systems",
        "Product",
        "A modular product launch for calm, organised work.",
        "MAKE ROOM FOR WHAT MATTERS",
        "BETTER\nSYSTEMS",
        "Less searching. More doing.",
        "INTRODUCING A CLEARER WORKSPACE",
        0xDFEAF3,
        0x1E304C,
        0x5369D8,
        0x93C7BD
    ),
    entry!(
        7,
        Grid,
        2,
        "play-date",
        "Play Date",
        "Events",
        "A bright, offbeat invitation for creative play.",
        "NO PERFECT IDEAS REQUIRED",
        "PLAY\nDATE",
        "An afternoon of making a happy mess.",
        "DROP IN / MAKE SOMETHING / TAKE IT HOME",
        0xF6DCD8,
        0x542F47,
        0xEE765E,
        0x9A9BCF
    ),
    entry!(
        8,
        Grid,
        3,
        "next-step",
        "Next Step",
        "Education",
        "A practical programme announcement with upward momentum.",
        "YOUR NEXT CHAPTER STARTS HERE",
        "NEXT\nSTEP",
        "A short course for a long-held idea.",
        "SIX WEEKS / SMALL GROUPS / BIG POSSIBILITIES",
        0xEFF1DE,
        0x263E3A,
        0xA7BC50,
        0xE2935C
    ),
    entry!(
        9,
        Botanical,
        0,
        "slow-growing",
        "Slow Growing",
        "Wellness",
        "Quiet branching foliage for thoughtful routines.",
        "A LITTLE EVERY DAY",
        "SLOW\nGROWING",
        "Good things take their own time.",
        "A FIELD GUIDE TO GENTLER DAYS",
        0xEBEBDD,
        0x304C3B,
        0x63876B,
        0xCAA97A
    ),
    entry!(
        10,
        Botanical,
        1,
        "flower-club",
        "Flower Club",
        "Community",
        "A joyful floral gathering with a generous central bloom.",
        "FRESH STEMS. NEW FRIENDS.",
        "FLOWER\nCLUB",
        "Make something lovely with your hands.",
        "MONTHLY / NO EXPERIENCE NEEDED",
        0xF8E4D8,
        0x693B3E,
        0xEB9471,
        0xB6BE83
    ),
    entry!(
        11,
        Botanical,
        2,
        "palm-house",
        "Palm House",
        "Food",
        "A tropical café identity made from sculptural fan leaves.",
        "AN EVERYDAY LITTLE ESCAPE",
        "PALM\nHOUSE",
        "Coffee, good food and a slower morning.",
        "OPEN DAILY / 08:00 TO 17:00",
        0xF0EAD7,
        0x164A40,
        0x55977B,
        0xD9AE51
    ),
    entry!(
        12,
        Botanical,
        3,
        "rooted",
        "Rooted",
        "Brand",
        "A ceramic-inspired identity for an earth-minded studio.",
        "MADE WITH CARE, MADE TO LAST",
        "ROOTED",
        "Objects with a place in your everyday.",
        "SMALL BATCH / THOUGHTFULLY MADE",
        0xDFD9C9,
        0x443B31,
        0xAF7558,
        0x819477
    ),
    entry!(
        13,
        Wave,
        0,
        "new-frequency",
        "New Frequency",
        "Culture",
        "Flowing sound bands for an experimental music release.",
        "TUNE INTO SOMETHING DIFFERENT",
        "NEW\nFREQUENCY",
        "Sounds from just beyond the familiar.",
        "FOUR TRACKS / ONE FRESH PERSPECTIVE",
        0xE8E2F0,
        0x302847,
        0x8B73C6,
        0xEBA56F
    ),
    entry!(
        14,
        Wave,
        1,
        "ridgeline",
        "Ridgeline",
        "Wellness",
        "An angular outdoor programme with a mountain rhythm.",
        "A DIFFERENT POINT OF VIEW",
        "RIDGELINE",
        "Take the long way. Notice more.",
        "WALK / BREATHE / BEGIN AGAIN",
        0xE2EBE7,
        0x244A4C,
        0x488A8B,
        0xC6B06A
    ),
    entry!(
        15,
        Wave,
        2,
        "still-water",
        "Still Water",
        "Editorial",
        "An unhurried reading series framed by soft ripples.",
        "NOTES FROM A QUIETER PLACE",
        "STILL\nWATER",
        "A collection of small observations.",
        "ESSAYS / CONVERSATIONS / SLOW IDEAS",
        0xE4ECEB,
        0x294A5A,
        0x7FA5B1,
        0xD4B588
    ),
    entry!(
        16,
        Wave,
        3,
        "in-motion",
        "In Motion",
        "Product",
        "Parallel flowing tracks for a product built to move.",
        "SMOOTH FROM THE FIRST STEP",
        "IN\nMOTION",
        "Less friction. A better daily rhythm.",
        "DESIGNED FOR THE WAY YOU WORK",
        0x212B33,
        0xF3EDDF,
        0xA4C4B4,
        0xDF945C
    ),
    entry!(
        17,
        Letters,
        0,
        "first-edition",
        "First Edition",
        "Editorial",
        "A numbered cover with confident editorial scale.",
        "IDEAS WORTH KEEPING",
        "FIRST\nEDITION",
        "A journal for things that deserve a second look.",
        "ISSUE 01 / INDEPENDENT THINKING",
        0xF2EBDC,
        0x262C30,
        0xC96950,
        0x98A3A3
    ),
    entry!(
        18,
        Letters,
        1,
        "make-room",
        "Make Room",
        "Brand",
        "A typographic studio manifesto with a repeating rhythm.",
        "SPACE FOR BETTER IDEAS",
        "MAKE\nROOM",
        "Clear a little space. Start something new.",
        "AN INDEPENDENT CREATIVE PRACTICE",
        0xEDE6F4,
        0x45345E,
        0xAB88C5,
        0xDD9C7D
    ),
    entry!(
        19,
        Letters,
        2,
        "and-company",
        "And Company",
        "Brand",
        "A warm partnership identity centred on an ampersand.",
        "BETTER THINGS HAPPEN TOGETHER",
        "AND\nCOMPANY",
        "Good people. Complementary ideas.",
        "STRATEGY / DESIGN / COLLABORATION",
        0xE5ECD9,
        0x314732,
        0x91A163,
        0xCB8E67
    ),
    entry!(
        20,
        Letters,
        3,
        "open-alphabet",
        "Open Alphabet",
        "Education",
        "A lively introduction to type, form and visual language.",
        "START WITH THE BASICS",
        "OPEN\nALPHABET",
        "A workshop in letters and possibilities.",
        "DRAW / ARRANGE / FIND YOUR VOICE",
        0xF4DFCB,
        0x583B31,
        0xE2824D,
        0x8DA1B6
    ),
    entry!(
        21,
        Architecture,
        0,
        "open-house",
        "Open House",
        "Events",
        "An arched invitation to visit a working studio.",
        "COME IN. LOOK AROUND.",
        "OPEN\nHOUSE",
        "Meet the makers behind the work.",
        "SATURDAY / STUDIO DOORS OPEN AT 10",
        0xE9DFD0,
        0x4A453C,
        0xBC8167,
        0x758A79
    ),
    entry!(
        22,
        Architecture,
        1,
        "common-ground",
        "Common Ground",
        "Community",
        "Stepped urban forms for a shared-space initiative.",
        "A PLACE FOR ALL OF US",
        "COMMON\nGROUND",
        "Bring an idea. Build something together.",
        "LOCAL PEOPLE / SHARED POSSIBILITIES",
        0xDDE6E6,
        0x294D53,
        0x598E97,
        0xDC9C6E
    ),
    entry!(
        23,
        Architecture,
        2,
        "form-and-space",
        "Form and Space",
        "Editorial",
        "An architectural study of proportion and repetition.",
        "STUDIES IN EVERYDAY STRUCTURE",
        "FORM\n& SPACE",
        "Finding the extraordinary in familiar places.",
        "ARCHITECTURE / OBJECTS / OBSERVATIONS",
        0xE7E1D8,
        0x3C3E42,
        0xAAA394,
        0xCB735D
    ),
    entry!(
        24,
        Architecture,
        3,
        "threshold",
        "Threshold",
        "Culture",
        "Nested doorways for an exhibition about new perspectives.",
        "STEP INTO ANOTHER PERSPECTIVE",
        "THRESHOLD",
        "New work at the edge of what comes next.",
        "A GROUP EXHIBITION / FREE ENTRY",
        0xF1E0D9,
        0x553E50,
        0xB083AC,
        0xDD9B78
    ),
    entry!(
        25,
        Ribbon,
        0,
        "fresh-cut",
        "Fresh Cut",
        "Brand",
        "Crisp diagonal paper strips for a new creative identity.",
        "A FRESH WAY TO SEE IT",
        "FRESH\nCUT",
        "Sharp ideas. Unexpected combinations.",
        "IDENTITY / PACKAGING / PRINT",
        0xF3EDDB,
        0x343D34,
        0xB5C365,
        0xDE8569
    ),
    entry!(
        26,
        Ribbon,
        1,
        "cross-talk",
        "Cross Talk",
        "Culture",
        "Intersecting ribbons for an exchange of ideas.",
        "TWO VOICES. NEW DIRECTIONS.",
        "CROSS\nTALK",
        "A conversation that changes the picture.",
        "LIVE DISCUSSIONS / OPEN QUESTIONS",
        0xE6E6F2,
        0x353557,
        0x8C89CC,
        0xE3A075
    ),
    entry!(
        27,
        Ribbon,
        2,
        "paper-trails",
        "Paper Trails",
        "Editorial",
        "Folded-paper geometry for stories that travel.",
        "FOLLOW A DIFFERENT THREAD",
        "PAPER\nTRAILS",
        "Notes, sketches and discoveries from the road.",
        "AN INDEPENDENT TRAVEL JOURNAL",
        0xEFE0CE,
        0x5A4435,
        0xC48258,
        0x9AA878
    ),
    entry!(
        28,
        Ribbon,
        3,
        "woven-together",
        "Woven Together",
        "Community",
        "Interlaced colour for a collaborative making project.",
        "MANY HANDS. ONE SHARED STORY.",
        "WOVEN\nTOGETHER",
        "Everyone brings something to the pattern.",
        "A COMMUNITY MAKING WEEKEND",
        0xE7E8D9,
        0x424A35,
        0x8EA06B,
        0xCE9476
    ),
    entry!(
        29,
        Ticket,
        0,
        "admit-one",
        "Admit One",
        "Events",
        "An elegant perforated ticket for a small live event.",
        "A GOOD EVENING STARTS HERE",
        "ADMIT\nONE",
        "Come for the show. Stay for the conversation.",
        "DOORS 19:00 / LIMITED CAPACITY",
        0xF1E5CD,
        0x403A35,
        0xCE7D54,
        0xA8A890
    ),
    entry!(
        30,
        Ticket,
        1,
        "weekend-pass",
        "Weekend Pass",
        "Events",
        "Layered passes for a whole weekend of discoveries.",
        "TWO DAYS. ENDLESS POSSIBILITIES.",
        "WEEKEND\nPASS",
        "Music, making and moments worth keeping.",
        "FRIDAY TO SUNDAY / EXPLORE THE PROGRAMME",
        0xE5E9F0,
        0x2F425D,
        0x7399C1,
        0xDBA076
    ),
    entry!(
        31,
        Ticket,
        2,
        "makers-mark",
        "Maker’s Mark",
        "Brand",
        "A bold stamp of care for an independent maker.",
        "PROUDLY MADE, PERSONALLY CHOSEN",
        "MAKER\nMARK",
        "A small collection with a human touch.",
        "DESIGNED / MADE / FINISHED BY HAND",
        0xEFE5D6,
        0x4F4235,
        0xB99362,
        0x7B9787
    ),
    entry!(
        32,
        Ticket,
        3,
        "meet-me-there",
        "Meet Me There",
        "Community",
        "A friendly meeting invitation with a dotted route.",
        "PUT SOMETHING GOOD ON THE CALENDAR",
        "MEET ME\nTHERE",
        "Old friends. New faces. One good reason to go.",
        "NEXT SATURDAY / SAVE YOUR PLACE",
        0xF3E1E1,
        0x5D3F54,
        0xC38CA8,
        0xD9AD6C
    ),
    entry!(
        33,
        Horizon,
        0,
        "daybreak",
        "Daybreak",
        "Wellness",
        "A geometric sunrise for a bright new beginning.",
        "START WITH A LITTLE LIGHT",
        "DAYBREAK",
        "A fresh routine for a gentler morning.",
        "MOVE / REST / MAKE SPACE",
        0xF1E9D6,
        0x594936,
        0xE3A653,
        0xB3A66B
    ),
    entry!(
        34,
        Horizon,
        1,
        "moon-garden",
        "Moon Garden",
        "Culture",
        "An evening landscape for a quiet cultural programme.",
        "THE CITY, AFTER THE LIGHT FADES",
        "MOON\nGARDEN",
        "An evening of sound, stories and stillness.",
        "TWILIGHT SESSIONS / OUTDOORS",
        0x242F48,
        0xF1EBD9,
        0xBCB3DA,
        0x789C9C
    ),
    entry!(
        35,
        Horizon,
        2,
        "far-from-here",
        "Far From Here",
        "Editorial",
        "Layered dunes for a travel essay or seasonal escape.",
        "LEAVE A LITTLE ROOM FOR WONDER",
        "FAR FROM\nHERE",
        "Places that stay with you after you leave.",
        "TRAVEL SLOWLY / LOOK CLOSELY",
        0xF3E2CD,
        0x664934,
        0xCE9163,
        0xA9AD84
    ),
    entry!(
        36,
        Horizon,
        3,
        "blue-hour",
        "Blue Hour",
        "Food",
        "A coastal evening identity for a relaxed restaurant.",
        "ONE MORE HOUR AT THE TABLE",
        "BLUE\nHOUR",
        "Seasonal plates. Something cold. Good company.",
        "DINNER BY THE WATER / FROM 17:00",
        0xDCE8ED,
        0x254D66,
        0x6C9DB5,
        0xD3B582
    ),
    entry!(
        37,
        Signal,
        0,
        "bright-ideas",
        "Bright Ideas",
        "Education",
        "A radiating workshop announcement with cheerful energy.",
        "A SMALL SPARK GOES A LONG WAY",
        "BRIGHT\nIDEAS",
        "Bring a question. Leave with a possibility.",
        "IDEA LAB / OPEN TO EVERYONE",
        0xF6EBCB,
        0x4A4730,
        0xD9B64E,
        0xB5BA82
    ),
    entry!(
        38,
        Signal,
        1,
        "sound-check",
        "Sound Check",
        "Culture",
        "A graphic sound meter for a local music showcase.",
        "TURN UP FOR SOMETHING NEW",
        "SOUND\nCHECK",
        "Fresh voices from your own city.",
        "LOCAL ARTISTS / LIVE SESSIONS",
        0x282E3A,
        0xF1ECDE,
        0xD49C77,
        0x91B5A7
    ),
    entry!(
        39,
        Signal,
        2,
        "on-the-radar",
        "On the Radar",
        "Editorial",
        "A measured visual scan of what is worth watching.",
        "A CURATED VIEW OF WHAT COMES NEXT",
        "ON THE\nRADAR",
        "People, projects and promising directions.",
        "THE MONTHLY EDIT / STAY CURIOUS",
        0xE0EBE4,
        0x284D45,
        0x719C86,
        0xD1A572
    ),
    entry!(
        40,
        Signal,
        3,
        "local-network",
        "Local Network",
        "Community",
        "A connected map for independent local businesses.",
        "GOOD THINGS, CLOSE TO HOME",
        "LOCAL\nNETWORK",
        "Find your people. Support your neighbourhood.",
        "SHOPS / STUDIOS / SMALL BUSINESSES",
        0xE9E3EF,
        0x443D62,
        0x9E8BBC,
        0xDDA37F
    ),
    entry!(
        41,
        Objects,
        0,
        "daily-dose",
        "Daily Dose",
        "Product",
        "A sculptural bottle composition for an everyday product.",
        "SMALL RITUALS. BETTER DAYS.",
        "DAILY\nDOSE",
        "Thoughtful essentials for an everyday routine.",
        "SIMPLY FORMULATED / BEAUTIFULLY USEFUL",
        0xE9EBDD,
        0x3C5040,
        0x91A382,
        0xD3B78F
    ),
    entry!(
        42,
        Objects,
        1,
        "coffee-break",
        "Coffee Break",
        "Food",
        "Warm stacked cups for a neighbourhood coffee offer.",
        "TAKE FIVE. MAKE IT GOOD.",
        "COFFEE\nBREAK",
        "A proper cup and a moment to yourself.",
        "FRESHLY BREWED / ALL DAY LONG",
        0xF0DFC9,
        0x543E31,
        0xC68F62,
        0x83988A
    ),
    entry!(
        43,
        Objects,
        2,
        "bookish",
        "Bookish",
        "Culture",
        "An open-book composition for a reading club or launch.",
        "ONE BOOK. MANY PERSPECTIVES.",
        "BOOKISH",
        "A gathering for readers with something to say.",
        "READ / MEET / START A CONVERSATION",
        0xE1E8ED,
        0x384E65,
        0x7B9BAF,
        0xD4A680
    ),
    entry!(
        44,
        Objects,
        3,
        "good-light",
        "Good Light",
        "Product",
        "A minimal lamp study for considered home objects.",
        "A LITTLE WARMTH GOES A LONG WAY",
        "GOOD\nLIGHT",
        "Everyday objects. A more inviting home.",
        "THE NEW COLLECTION / DESIGNED TO STAY",
        0xEEE6D8,
        0x504A3C,
        0xBEA376,
        0x8FA59A
    ),
    entry!(
        45,
        Mosaic,
        0,
        "pieces-of-us",
        "Pieces of Us",
        "Culture",
        "A fragmented collage for a collective exhibition.",
        "DIFFERENT STORIES. ONE BIG PICTURE.",
        "PIECES\nOF US",
        "New perspectives from a community of artists.",
        "COLLECTIVE WORK / OPEN EXHIBITION",
        0xF0E2D9,
        0x5B4344,
        0xCD8F79,
        0x9EAD8E
    ),
    entry!(
        46,
        Mosaic,
        1,
        "round-about",
        "Round About",
        "Brand",
        "Quarter-circle tiles for a playful, versatile identity.",
        "A LITTLE LESS STRAIGHTFORWARD",
        "ROUND\nABOUT",
        "Fresh angles on everyday possibilities.",
        "A STUDIO FOR CURIOUS BRANDS",
        0xE4EADF,
        0x344B45,
        0x8DAF95,
        0xD1A067
    ),
    entry!(
        47,
        Mosaic,
        2,
        "inner-world",
        "Inner World",
        "Wellness",
        "Nested colour blocks for a reflective personal practice.",
        "MAKE A LITTLE SPACE INSIDE",
        "INNER\nWORLD",
        "A few quiet minutes can change the day.",
        "JOURNAL / NOTICE / BEGIN AGAIN",
        0xEAE1EF,
        0x514064,
        0xAB92C0,
        0xD8AC86
    ),
    entry!(
        48,
        Mosaic,
        3,
        "small-wonders",
        "Small Wonders",
        "Education",
        "A pixel-like maze for curious minds of all ages.",
        "LOOK CLOSER. THERE IS MORE.",
        "SMALL\nWONDERS",
        "Experiments, discoveries and useful surprises.",
        "A HANDS-ON LEARNING CLUB",
        0xEEECCC,
        0x4D5131,
        0xA8B45C,
        0xD69760
    ),
    entry!(
        49,
        Frame,
        0,
        "in-good-company",
        "In Good Company",
        "Brand",
        "An open frame for a gracious business introduction.",
        "GOOD WORK STARTS WITH GOOD PEOPLE",
        "IN GOOD\nCOMPANY",
        "Thoughtful partnerships. Work we believe in.",
        "SAY HELLO / START SOMETHING TOGETHER",
        0xE7EADF,
        0x3E5244,
        0x97AA8D,
        0xC9A582
    ),
    entry!(
        50,
        Frame,
        1,
        "collected",
        "Collected",
        "Editorial",
        "A nested frame for a carefully chosen seasonal edit.",
        "FEWER THINGS. BETTER CHOICES.",
        "COLLECTED",
        "Objects, places and ideas worth holding onto.",
        "THE SEASONAL EDIT / CONSIDERED LIVING",
        0xEEE4D6,
        0x534437,
        0xB79671,
        0x8C9D91
    ),
    entry!(
        51,
        Frame,
        2,
        "a-new-angle",
        "A New Angle",
        "Events",
        "An offset exhibition frame with an unexpected opening.",
        "SEE THE FAMILIAR DIFFERENTLY",
        "A NEW\nANGLE",
        "Fresh work. Unusual perspectives. Open minds.",
        "NEW WORK / STUDIO EXHIBITION",
        0xE5E3EF,
        0x48405F,
        0x9E90C1,
        0xD59A7A
    ),
    entry!(
        52,
        Frame,
        3,
        "one-good-year",
        "One Good Year",
        "Community",
        "A celebratory framed emblem for a year of shared progress.",
        "LOOK WHAT WE MADE TOGETHER",
        "ONE GOOD\nYEAR",
        "A celebration of small steps and shared successes.",
        "THANK YOU FOR BEING PART OF IT",
        0xF1E6CF,
        0x494B37,
        0xB6AA65,
        0xC99075
    ),
];

pub fn categories() -> &'static [&'static str] {
    &[
        "Brand",
        "Community",
        "Culture",
        "Education",
        "Editorial",
        "Events",
        "Food",
        "Product",
        "Wellness",
    ]
}

pub fn find(id: &str) -> Option<&'static Template> {
    CATALOG.iter().find(|template| template.id == id)
}

/// Build at the requested size without allocating a page-sized raster surface.
pub fn build(id: &str, width: f32, height: f32, dpi: f32) -> Result<Document, String> {
    let template = find(id).ok_or_else(|| format!("Unknown template: {id}"))?;
    if !width.is_finite()
        || !height.is_finite()
        || !dpi.is_finite()
        || width < 16.0
        || height < 16.0
        || dpi <= 0.0
    {
        return Err(
            "Choose finite dimensions of at least 16 pixels and a positive resolution".into(),
        );
    }
    let mut doc = Document::new(template.name, 1.0, 1.0, dpi);
    doc.width = width;
    doc.height = height;
    doc.artboards = vec![Artboard::new(0, Pt::ZERO, Pt::new(width, height))];
    doc.layers = vec![
        Layer::vector("Paper"),
        Layer::vector("Original artwork"),
        Layer::vector("Editable copy"),
    ];
    doc.artboards[0].name = template.name.into();
    let colors = template.palette.map(Rgba::from_hex);
    let mut page = Drawing {
        doc,
        area: Area::new(0.0, 0.0, width, height),
        layer: 0,
    };
    page.rect(0.0, 0.0, 1.0, 1.0, colors[0]);
    page.layer = 1;
    let wide = width / height > 1.3;
    let (art, title, body) = if wide {
        match template.variant {
            1 => (
                Area::new(0.07, 0.17, 0.37, 0.66),
                Area::new(0.50, 0.22, 0.43, 0.32),
                Area::new(0.50, 0.62, 0.40, 0.13),
            ),
            2 => (
                Area::new(0.56, 0.13, 0.37, 0.69),
                Area::new(0.07, 0.17, 0.44, 0.40),
                Area::new(0.07, 0.66, 0.39, 0.12),
            ),
            3 => (
                Area::new(0.57, 0.19, 0.36, 0.60),
                Area::new(0.07, 0.26, 0.45, 0.34),
                Area::new(0.07, 0.68, 0.40, 0.12),
            ),
            _ => (
                Area::new(0.54, 0.14, 0.39, 0.70),
                Area::new(0.07, 0.22, 0.42, 0.34),
                Area::new(0.07, 0.64, 0.39, 0.12),
            ),
        }
    } else {
        match template.variant {
            1 => (
                Area::new(0.12, 0.14, 0.76, 0.35),
                Area::new(0.07, 0.55, 0.86, 0.20),
                Area::new(0.07, 0.81, 0.82, 0.07),
            ),
            2 => (
                Area::new(0.07, 0.47, 0.44, 0.31),
                Area::new(0.07, 0.15, 0.86, 0.24),
                Area::new(0.56, 0.55, 0.37, 0.19),
            ),
            3 => (
                Area::new(0.15, 0.36, 0.70, 0.36),
                Area::new(0.07, 0.14, 0.86, 0.18),
                Area::new(0.07, 0.80, 0.82, 0.08),
            ),
            _ => (
                Area::new(0.10, 0.43, 0.80, 0.33),
                Area::new(0.07, 0.15, 0.86, 0.24),
                Area::new(0.07, 0.82, 0.82, 0.07),
            ),
        }
    };
    let full = page.area;
    let short = width.min(height);
    // Structural accents change with the composition, not just its palette.
    match template.variant {
        0 => page.rect(
            0.07,
            0.108,
            if wide { 0.13 } else { 0.24 },
            0.003,
            colors[2],
        ),
        1 => page.rect(0.95, 0.15, 0.007, 0.70, colors[2]),
        2 => {
            page.circle(0.08, 0.865, 0.011, colors[2]);
            page.circle(0.115, 0.865, 0.011, colors[3]);
        }
        _ => page.rect(0.07, 0.88, 0.10, 0.006, colors[2]),
    }
    page.area = full.child(art).square();
    artwork(&mut page, template, colors);
    page.area = full;
    page.layer = 2;
    let serif = matches!(
        template.family,
        Layout::Botanical | Layout::Horizon | Layout::Objects | Layout::Frame
    );
    page.text(
        "Section label",
        template.kicker,
        full.child(Area::new(0.07, 0.055, 0.86, 0.038)),
        short * 0.015,
        colors[1],
        false,
        false,
        false,
    );
    page.text(
        "Headline",
        template.title,
        full.child(title),
        short * if wide { 0.13 } else { 0.10 },
        colors[1],
        true,
        serif,
        false,
    );
    page.text(
        "Supporting copy",
        template.body,
        full.child(body),
        short * 0.026,
        colors[1],
        false,
        serif,
        true,
    );
    page.text(
        "Details",
        template.footer,
        full.child(Area::new(0.07, 0.94, 0.86, 0.033)),
        short * 0.0125,
        colors[1],
        false,
        false,
        false,
    );
    Ok(page.doc)
}

#[derive(Clone, Copy)]
struct Area {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}
impl Area {
    fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    fn point(self, x: f32, y: f32) -> Pt {
        Pt::new(self.x + x * self.w, self.y + y * self.h)
    }
    fn child(self, area: Self) -> Self {
        Self::new(
            self.x + area.x * self.w,
            self.y + area.y * self.h,
            area.w * self.w,
            area.h * self.h,
        )
    }
    fn square(self) -> Self {
        let side = self.w.min(self.h);
        Self::new(
            self.x + (self.w - side) * 0.5,
            self.y + (self.h - side) * 0.5,
            side,
            side,
        )
    }
}

struct Drawing {
    doc: Document,
    area: Area,
    layer: usize,
}
impl Drawing {
    fn shape(&mut self, name: &str, geom: Geom, fill: Rgba, stroke: Option<Stroke>) {
        let mut shape = Shape::new(
            geom,
            Style {
                fill: Fill::Solid(fill),
                stroke,
            },
        );
        shape.name = name.into();
        self.doc.layers[self.layer]
            .kind
            .shapes_mut()
            .unwrap()
            .push(shape);
    }
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        self.shape(
            "Colour block",
            Geom::Rect {
                origin: self.area.point(x, y),
                size: Pt::new(w * self.area.w, h * self.area.h),
                radius: 0.0,
            },
            color,
            None,
        );
    }
    fn ellipse(&mut self, x: f32, y: f32, rx: f32, ry: f32, color: Rgba) {
        self.shape(
            "Ellipse",
            Geom::Ellipse {
                center: self.area.point(x, y),
                radii: Pt::new(rx * self.area.w, ry * self.area.h),
            },
            color,
            None,
        );
    }
    fn circle(&mut self, x: f32, y: f32, r: f32, color: Rgba) {
        self.ellipse(x, y, r, r, color);
    }
    fn ring(&mut self, x: f32, y: f32, r: f32, width: f32, color: Rgba) {
        self.shape(
            "Ring",
            Geom::Ellipse {
                center: self.area.point(x, y),
                radii: Pt::new(r * self.area.w, r * self.area.h),
            },
            Rgba::TRANSPARENT,
            Some(Stroke {
                color,
                width: width * self.area.w,
                ..Default::default()
            }),
        );
    }
    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, color: Rgba) {
        self.shape(
            "Line",
            Geom::Line {
                a: self.area.point(a.0, a.1),
                b: self.area.point(b.0, b.1),
            },
            Rgba::TRANSPARENT,
            Some(Stroke {
                color,
                width: width * self.area.w,
                ..Default::default()
            }),
        );
    }
    fn poly(&mut self, points: &[(f32, f32)], color: Rgba) {
        self.shape(
            "Paper form",
            Geom::Poly {
                contours: vec![points.iter().map(|&(x, y)| self.area.point(x, y)).collect()],
                winding: false,
            },
            color,
            None,
        );
    }
    fn star(&mut self, x: f32, y: f32, r: f32, inner: f32, points: u32, color: Rgba) {
        self.shape(
            "Seal",
            Geom::Star {
                center: self.area.point(x, y),
                outer: Pt::new(r * self.area.w, r * self.area.h),
                inner,
                points,
            },
            color,
            None,
        );
    }
    fn wave(&mut self, y: f32, height: f32, frequency: f32, phase: f32, color: Rgba) {
        let mut points = Vec::with_capacity(66);
        for i in 0..=32 {
            let x = 0.04 + i as f32 / 32.0 * 0.92;
            points.push((x, y + (x * frequency + phase).sin() * 0.045));
        }
        for i in (0..=32).rev() {
            let x = 0.04 + i as f32 / 32.0 * 0.92;
            points.push((x, y + height + (x * frequency + phase).sin() * 0.045));
        }
        self.poly(&points, color);
    }
    fn text(
        &mut self,
        name: &str,
        content: &str,
        area: Area,
        px: f32,
        color: Rgba,
        bold: bool,
        serif: bool,
        wrap: bool,
    ) {
        let mut run = TypeRun {
            origin: Pt::ZERO,
            content: content.into(),
            px: px.max(1.0),
            font: font_path(bold, serif).into(),
            ..Default::default()
        };
        if wrap {
            let mut lines = Vec::new();
            let mut line = String::new();
            for word in content.split_whitespace() {
                let candidate = if line.is_empty() {
                    word.into()
                } else {
                    format!("{line} {word}")
                };
                run.content = candidate.clone();
                if !line.is_empty() && crate::text::measure(&run).0 > area.w {
                    lines.push(line);
                    line = word.into();
                } else {
                    line = candidate;
                }
            }
            if !line.is_empty() {
                lines.push(line);
            }
            run.content = lines.join("\n");
        }
        run.leading = run.px * 1.08;
        let measured = crate::text::measure(&run);
        let scale = (area.w / measured.0.max(1.0))
            .min(area.h / measured.1.max(1.0))
            .min(1.0);
        run.px *= scale;
        run.leading *= scale;
        // The text renderer has a one-pixel floor. On icon-sized custom
        // canvases, keep the headline and artwork instead of overflowing with
        // secondary copy that cannot be read at that size.
        if run.px < 1.0 && self.layer == 2 && name != "Headline" {
            return;
        }
        run.origin = Pt::new(area.x, area.y + run.px);
        let mut geometry = Geom::Text(run);
        crate::text::fill_contours(&mut geometry);
        let bounds = geometry.bbox();
        let inset = if self.layer == 1 {
            Pt::new(
                (area.w - bounds.width()) * 0.5,
                (area.h - bounds.height()) * 0.5,
            )
        } else {
            Pt::ZERO
        };
        geometry.translate(Pt::new(area.x - bounds.min.x, area.y - bounds.min.y) + inset);
        self.shape(name, geometry, color, None);
    }
}

fn font_path(bold: bool, serif: bool) -> &'static str {
    static FONTS: OnceLock<[String; 3]> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        [
            "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
            "/usr/share/fonts/liberation/LiberationSerif-Regular.ttf",
        ]
        .map(|preferred| {
            if std::path::Path::new(preferred).is_file() {
                preferred.into()
            } else {
                crate::text::default_path()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default()
            }
        })
    });
    &fonts[if serif {
        2
    } else if bold {
        1
    } else {
        0
    }]
}

fn artwork(d: &mut Drawing, t: &Template, c: [Rgba; 4]) {
    let [paper, ink, a, b] = c;
    let tau = std::f32::consts::TAU;
    match (t.family, t.variant) {
        (Layout::Orbit, 0) => {
            for i in 0..7 {
                d.ring(
                    0.5,
                    0.5,
                    0.44 - i as f32 * 0.057,
                    0.016,
                    if i % 2 == 0 { a } else { b },
                );
            }
            d.circle(0.73, 0.21, 0.08, paper);
            d.circle(0.73, 0.21, 0.052, a);
        }
        (Layout::Orbit, 1) => {
            d.circle(0.5, 0.5, 0.24, a);
            for i in 0..8 {
                let v = i as f32 / 8.0 * tau;
                d.circle(
                    0.5 + v.cos() * 0.36,
                    0.5 + v.sin() * 0.36,
                    0.045 + 0.018 * (i % 3) as f32,
                    if i % 2 == 0 { b } else { ink },
                );
            }
        }
        (Layout::Orbit, 2) => {
            for (x, y, color) in [(0.36, 0.36, a), (0.64, 0.36, b), (0.5, 0.64, ink)] {
                d.ring(x, y, 0.25, 0.043, color);
            }
            d.circle(0.5, 0.49, 0.07, paper);
        }
        (Layout::Orbit, _) => {
            for i in 0..36 {
                let v = i as f32 / 36.0 * tau;
                d.line(
                    (0.5 + v.cos() * 0.23, 0.5 + v.sin() * 0.23),
                    (0.5 + v.cos() * 0.44, 0.5 + v.sin() * 0.44),
                    0.009,
                    if i % 4 == 0 { b } else { a },
                );
            }
            d.circle(0.5, 0.5, 0.14, b);
            d.circle(0.56, 0.46, 0.13, paper);
        }
        (Layout::Grid, 0) => {
            for y in 0..3 {
                for x in 0..3 {
                    let px = 0.065 + x as f32 * 0.3;
                    let py = 0.065 + y as f32 * 0.3;
                    match (x + y) % 3 {
                        0 => d.rect(px, py, 0.27, 0.27, a),
                        1 => d.circle(px + 0.135, py + 0.135, 0.135, b),
                        _ => d.poly(
                            &[(px, py + 0.27), (px + 0.27, py + 0.27), (px + 0.27, py)],
                            ink,
                        ),
                    }
                }
            }
        }
        (Layout::Grid, 1) => {
            for i in 0..5 {
                let x = 0.075 + i as f32 * 0.18;
                let h = [0.43, 0.65, 0.81, 0.58, 0.32][i];
                d.rect(x, 0.91 - h, 0.145, h, if i % 2 == 0 { a } else { b });
                for j in 0..4 {
                    d.rect(
                        x + 0.027,
                        0.92 - h + j as f32 * h / 4.0,
                        0.035,
                        0.027,
                        paper,
                    );
                }
            }
        }
        (Layout::Grid, 2) => {
            for i in 0..4 {
                for j in 0..4 {
                    let x = 0.04 + i as f32 * 0.23;
                    let y = 0.06 + j as f32 * 0.21;
                    if (i + j) % 3 != 0 {
                        d.rect(x, y, 0.19, 0.16, if (i + j) % 2 == 0 { a } else { b });
                    } else {
                        d.circle(x + 0.095, y + 0.08, 0.06, ink);
                    }
                }
            }
        }
        (Layout::Grid, _) => {
            for i in 0..4 {
                let x = 0.08 + i as f32 * 0.21;
                let y = 0.70 - i as f32 * 0.16;
                d.poly(
                    &[
                        (x, y + 0.2),
                        (x, y + 0.065),
                        (x + 0.06, y + 0.065),
                        (x + 0.06, y),
                        (x + 0.17, y + 0.1),
                        (x + 0.06, y + 0.2),
                        (x + 0.06, y + 0.135),
                        (x + 0.04, y + 0.135),
                        (x + 0.04, y + 0.2),
                    ],
                    if i % 2 == 0 { a } else { b },
                );
            }
        }
        (Layout::Botanical, 0) => {
            d.line((0.5, 0.91), (0.5, 0.13), 0.012, ink);
            for i in 0..4 {
                let y = 0.25 + i as f32 * 0.16;
                d.poly(
                    &[
                        (0.5, y + 0.1),
                        (0.21, y + 0.025),
                        (0.14, y - 0.085),
                        (0.34, y - 0.06),
                    ],
                    a,
                );
                d.poly(
                    &[
                        (0.5, y + 0.07),
                        (0.8, y - 0.035),
                        (0.86, y - 0.14),
                        (0.66, y - 0.09),
                    ],
                    b,
                );
            }
        }
        (Layout::Botanical, 1) => {
            d.line((0.5, 0.86), (0.5, 0.41), 0.015, ink);
            for i in 0..7 {
                let v = i as f32 / 7.0 * tau;
                d.circle(
                    0.5 + v.cos() * 0.19,
                    0.39 + v.sin() * 0.19,
                    0.145,
                    if i % 2 == 0 { a } else { b },
                );
            }
            d.circle(0.5, 0.39, 0.12, ink);
            d.ellipse(0.35, 0.76, 0.14, 0.06, b);
        }
        (Layout::Botanical, 2) => {
            d.line((0.5, 0.91), (0.5, 0.55), 0.018, ink);
            for i in 0..11 {
                let angle = std::f32::consts::PI * (1.12 + i as f32 * 0.076);
                let (x, y) = (0.5 + angle.cos() * 0.44, 0.59 + angle.sin() * 0.47);
                d.poly(
                    &[(0.5, 0.69), (x - 0.025, y), (x + 0.025, y), (0.54, 0.69)],
                    if i % 2 == 0 { a } else { b },
                );
            }
        }
        (Layout::Botanical, _) => {
            d.rect(0.1, 0.68, 0.3, 0.2, a);
            d.ellipse(0.25, 0.68, 0.15, 0.06, a);
            d.rect(0.57, 0.53, 0.31, 0.35, b);
            d.ellipse(0.725, 0.53, 0.155, 0.07, b);
            d.line((0.25, 0.65), (0.22, 0.28), 0.013, ink);
            d.ellipse(0.22, 0.26, 0.06, 0.15, ink);
            d.line((0.725, 0.51), (0.66, 0.2), 0.012, ink);
            d.ellipse(0.73, 0.24, 0.12, 0.055, a);
            d.ellipse(0.6, 0.34, 0.09, 0.05, ink);
        }
        (Layout::Wave, 0) => {
            for i in 0..7 {
                d.wave(
                    0.12 + i as f32 * 0.11,
                    0.063,
                    8.0,
                    i as f32 * 0.6,
                    if i % 2 == 0 { a } else { b },
                );
            }
        }
        (Layout::Wave, 1) => {
            for i in (0..5).rev() {
                let y = 0.12 + i as f32 * 0.145;
                d.poly(
                    &[
                        (0.04, y + 0.18),
                        (0.25, y),
                        (0.45, y + 0.16),
                        (0.7, y + 0.035),
                        (0.96, y + 0.22),
                        (0.96, y + 0.29),
                        (0.7, y + 0.105),
                        (0.45, y + 0.23),
                        (0.25, y + 0.07),
                        (0.04, y + 0.25),
                    ],
                    if i % 2 == 0 { a } else { b },
                );
            }
        }
        (Layout::Wave, 2) => {
            for i in 0..8 {
                d.ellipse(
                    0.5,
                    0.5,
                    0.44 - i as f32 * 0.052,
                    0.32 - i as f32 * 0.037,
                    if i % 2 == 0 { a } else { paper },
                );
            }
            d.circle(0.51, 0.48, 0.04, b);
        }
        (Layout::Wave, _) => {
            for i in 0..9 {
                d.wave(
                    0.1 + i as f32 * 0.092,
                    0.018,
                    5.0,
                    i as f32 * 0.18,
                    if i % 3 == 0 { b } else { a },
                );
            }
        }
        (Layout::Letters, variant) => {
            let area = d.area;
            match variant {
                0 => {
                    d.rect(0.03, 0.04, 0.94, 0.91, a);
                    d.text(
                        "Edition number",
                        "01",
                        area.child(Area::new(0.08, 0.12, 0.82, 0.72)),
                        area.w * 0.79,
                        paper,
                        true,
                        false,
                        false,
                    );
                }
                1 => {
                    for i in 0..4 {
                        d.text(
                            "Make motif",
                            "MAKE",
                            area.child(Area::new(
                                0.05 + i as f32 * 0.02,
                                0.06 + i as f32 * 0.215,
                                0.87,
                                0.19,
                            )),
                            area.w * 0.23,
                            if i % 2 == 0 { a } else { b },
                            true,
                            false,
                            false,
                        );
                    }
                }
                2 => {
                    d.circle(0.5, 0.49, 0.42, b);
                    d.text(
                        "Ampersand",
                        "&",
                        area.child(Area::new(0.2, 0.1, 0.63, 0.76)),
                        area.w * 0.85,
                        ink,
                        false,
                        true,
                        false,
                    );
                }
                _ => {
                    for (i, letter) in ["A", "B", "C"].iter().enumerate() {
                        let x = 0.04 + i as f32 * 0.285;
                        let y = 0.06 + i as f32 * 0.18;
                        d.rect(x, y, 0.31, 0.46, if i % 2 == 0 { a } else { b });
                        d.text(
                            "Letter study",
                            letter,
                            area.child(Area::new(x + 0.02, y + 0.03, 0.27, 0.38)),
                            area.w * 0.39,
                            paper,
                            true,
                            false,
                            false,
                        );
                    }
                }
            }
        }
        (Layout::Architecture, 0) => {
            for i in 0..5 {
                let x = 0.08 + i as f32 * 0.075;
                let w = 0.84 - i as f32 * 0.15;
                d.rect(x, 0.46, w, 0.46, if i % 2 == 0 { a } else { paper });
                d.ellipse(
                    0.5,
                    0.46,
                    w * 0.5,
                    w * 0.5,
                    if i % 2 == 0 { a } else { paper },
                );
            }
            d.circle(0.5, 0.60, 0.09, b);
        }
        (Layout::Architecture, 1) => {
            for i in 0..5 {
                let x = 0.06 + i as f32 * 0.175;
                let h = 0.16 + i as f32 * 0.16;
                d.rect(x, 0.9 - h, 0.175, h, if i % 2 == 0 { a } else { b });
                d.rect(x + 0.04, 0.91 - h, 0.09, 0.015, paper);
            }
        }
        (Layout::Architecture, 2) => {
            d.rect(0.05, 0.16, 0.9, 0.1, b);
            d.rect(0.05, 0.81, 0.9, 0.08, b);
            for i in 0..5 {
                d.rect(0.10 + i as f32 * 0.17, 0.25, 0.12, 0.56, a);
                d.rect(0.10 + i as f32 * 0.17, 0.26, 0.025, 0.54, ink);
            }
        }
        (Layout::Architecture, _) => {
            for i in (0..5).rev() {
                let x = 0.07 + i as f32 * 0.14;
                let y = 0.11 + i as f32 * 0.09;
                d.rect(x, y + 0.13, 0.3, 0.35, if i % 2 == 0 { a } else { b });
                d.circle(x + 0.15, y + 0.13, 0.15, if i % 2 == 0 { a } else { b });
                d.rect(x + 0.055, y + 0.13, 0.19, 0.35, paper);
                d.circle(x + 0.15, y + 0.13, 0.095, paper);
            }
        }
        (Layout::Ribbon, 0) => {
            for i in 0..5 {
                let x = 0.05 + i as f32 * 0.17;
                d.poly(
                    &[(x, 0.17), (x + 0.11, 0.08), (x + 0.11, 0.8), (x, 0.91)],
                    if i % 2 == 0 { a } else { b },
                );
            }
        }
        (Layout::Ribbon, 1) => {
            d.poly(&[(0.07, 0.14), (0.24, 0.06), (0.93, 0.78), (0.77, 0.94)], a);
            d.poly(&[(0.78, 0.06), (0.94, 0.24), (0.22, 0.94), (0.06, 0.77)], b);
            d.poly(
                &[(0.39, 0.38), (0.51, 0.27), (0.67, 0.45), (0.54, 0.57)],
                ink,
            );
        }
        (Layout::Ribbon, 2) => {
            d.poly(&[(0.08, 0.15), (0.89, 0.1), (0.57, 0.43)], a);
            d.poly(&[(0.08, 0.15), (0.57, 0.43), (0.29, 0.84)], b);
            d.poly(&[(0.29, 0.84), (0.57, 0.43), (0.93, 0.67)], ink);
            d.poly(&[(0.29, 0.84), (0.93, 0.67), (0.79, 0.92)], a);
        }
        (Layout::Ribbon, _) => {
            for i in 0..5 {
                let pos = 0.07 + i as f32 * 0.18;
                d.rect(pos, 0.06, 0.105, 0.88, a);
                d.rect(0.06, pos, 0.88, 0.105, b);
            }
            for y in 0..5 {
                for x in 0..5 {
                    if (x + y) % 2 == 0 {
                        d.rect(
                            0.07 + x as f32 * 0.18,
                            0.07 + y as f32 * 0.18,
                            0.105,
                            0.105,
                            a,
                        );
                    }
                }
            }
        }
        (Layout::Ticket, 0) => {
            d.rect(0.04, 0.22, 0.92, 0.56, a);
            for i in 0..12 {
                d.circle(0.10 + i as f32 * 0.073, 0.22, 0.017, paper);
                d.circle(0.10 + i as f32 * 0.073, 0.78, 0.017, paper);
            }
            for i in 0..9 {
                d.rect(0.76, 0.29 + i as f32 * 0.047, 0.014, 0.022, paper);
            }
            d.star(0.36, 0.50, 0.19, 0.55, 8, paper);
        }
        (Layout::Ticket, 1) => {
            for i in 0..3 {
                let x = 0.08 + i as f32 * 0.15;
                let y = 0.1 + i as f32 * 0.14;
                d.rect(x, y, 0.62, 0.50, if i % 2 == 0 { a } else { b });
                d.ring(x + 0.15, y + 0.24, 0.08, 0.02, paper);
                d.rect(x + 0.30, y + 0.16, 0.23, 0.025, paper);
                d.rect(x + 0.30, y + 0.25, 0.16, 0.025, paper);
            }
        }
        (Layout::Ticket, 2) => {
            d.star(0.5, 0.5, 0.45, 0.87, 24, a);
            d.ring(0.5, 0.5, 0.32, 0.014, paper);
            d.star(0.5, 0.5, 0.23, 0.44, 5, paper);
        }
        (Layout::Ticket, _) => {
            for i in 0..24 {
                let angle = i as f32 / 24.0 * tau;
                d.circle(0.5 + angle.cos() * 0.41, 0.5 + angle.sin() * 0.41, 0.012, a);
            }
            d.poly(
                &[
                    (0.25, 0.51),
                    (0.49, 0.25),
                    (0.73, 0.51),
                    (0.60, 0.51),
                    (0.60, 0.75),
                    (0.38, 0.75),
                    (0.38, 0.51),
                ],
                b,
            );
            d.circle(0.50, 0.47, 0.045, paper);
        }
        (Layout::Horizon, 0) => {
            d.circle(0.5, 0.41, 0.30, a);
            for i in 0..7 {
                d.rect(
                    0.04,
                    0.48 + i as f32 * 0.055,
                    0.92,
                    0.034,
                    if i % 2 == 0 { paper } else { b },
                );
            }
        }
        (Layout::Horizon, 1) => {
            d.circle(0.68, 0.25, 0.18, b);
            d.circle(0.74, 0.21, 0.17, paper);
            d.poly(
                &[
                    (0.05, 0.86),
                    (0.05, 0.62),
                    (0.27, 0.42),
                    (0.57, 0.69),
                    (0.78, 0.51),
                    (0.95, 0.63),
                    (0.95, 0.86),
                ],
                a,
            );
            d.poly(
                &[
                    (0.05, 0.91),
                    (0.05, 0.8),
                    (0.38, 0.64),
                    (0.75, 0.80),
                    (0.95, 0.69),
                    (0.95, 0.91),
                ],
                b,
            );
        }
        (Layout::Horizon, 2) => {
            d.circle(0.75, 0.21, 0.1, b);
            for i in 0..5 {
                d.wave(
                    0.35 + i as f32 * 0.10,
                    0.12,
                    4.0,
                    i as f32 * 0.55,
                    if i % 2 == 0 { a } else { b },
                );
            }
        }
        (Layout::Horizon, _) => {
            d.rect(0.14, 0.44, 0.72, 0.43, a);
            d.circle(0.5, 0.44, 0.36, a);
            d.circle(0.5, 0.38, 0.13, b);
            for i in 0..5 {
                d.rect(0.15, 0.55 + i as f32 * 0.057, 0.70, 0.017, paper);
            }
        }
        (Layout::Signal, 0) => {
            for i in 0..16 {
                let v = i as f32 / 16.0 * tau;
                d.poly(
                    &[
                        (0.5 + v.cos() * 0.21, 0.5 + v.sin() * 0.21),
                        (0.5 + (v + 0.08).cos() * 0.45, 0.5 + (v + 0.08).sin() * 0.45),
                        (0.5 + (v - 0.08).cos() * 0.45, 0.5 + (v - 0.08).sin() * 0.45),
                    ],
                    if i % 2 == 0 { a } else { b },
                );
            }
            d.circle(0.5, 0.5, 0.14, ink);
        }
        (Layout::Signal, 1) => {
            for i in 0..13 {
                let h = 0.12 + ((i as f32 * 1.13).sin().abs()) * 0.68;
                d.rect(
                    0.04 + i as f32 * 0.073,
                    0.5 - h * 0.5,
                    0.04,
                    h,
                    if i % 3 == 0 { b } else { a },
                );
            }
        }
        (Layout::Signal, 2) => {
            for i in 1..5 {
                d.ring(0.5, 0.5, i as f32 * 0.1, 0.01, a);
            }
            d.line((0.06, 0.5), (0.94, 0.5), 0.005, a);
            d.line((0.5, 0.06), (0.5, 0.94), 0.005, a);
            d.poly(&[(0.5, 0.5), (0.78, 0.17), (0.92, 0.36)], b);
            d.circle(0.28, 0.36, 0.035, ink);
            d.circle(0.69, 0.70, 0.048, ink);
        }
        (Layout::Signal, _) => {
            let points = [
                (0.17, 0.22),
                (0.52, 0.1),
                (0.86, 0.33),
                (0.76, 0.77),
                (0.41, 0.88),
                (0.13, 0.63),
                (0.47, 0.46),
            ];
            for &(x, y) in &points[..6] {
                d.line((x, y), points[6], 0.014, a);
            }
            for i in 0..6 {
                d.line(points[i], points[(i + 1) % 6], 0.012, b);
            }
            for (i, &(x, y)) in points.iter().enumerate() {
                d.circle(
                    x,
                    y,
                    if i == 6 { 0.11 } else { 0.065 },
                    if i % 2 == 0 { a } else { b },
                );
            }
        }
        (Layout::Objects, 0) => {
            d.rect(0.33, 0.27, 0.34, 0.59, a);
            d.ellipse(0.5, 0.27, 0.17, 0.10, a);
            d.rect(0.40, 0.11, 0.20, 0.11, ink);
            d.rect(0.36, 0.43, 0.28, 0.26, paper);
            d.circle(0.5, 0.54, 0.075, b);
            d.rect(0.41, 0.65, 0.18, 0.014, ink);
        }
        (Layout::Objects, 1) => {
            d.ring(0.73, 0.53, 0.135, 0.036, a);
            d.poly(&[(0.22, 0.37), (0.74, 0.37), (0.68, 0.78), (0.3, 0.78)], a);
            d.ellipse(0.48, 0.37, 0.26, 0.063, b);
            d.ellipse(0.48, 0.82, 0.38, 0.045, b);
            for i in 0..3 {
                d.line(
                    (0.34 + i as f32 * 0.14, 0.25),
                    (0.38 + i as f32 * 0.14, 0.1),
                    0.018,
                    ink,
                );
            }
        }
        (Layout::Objects, 2) => {
            d.poly(&[(0.07, 0.20), (0.45, 0.27), (0.5, 0.88), (0.07, 0.78)], a);
            d.poly(&[(0.5, 0.27), (0.93, 0.17), (0.93, 0.78), (0.5, 0.88)], b);
            for i in 0..6 {
                d.line(
                    (0.15, 0.32 + i as f32 * 0.066),
                    (0.39, 0.37 + i as f32 * 0.066),
                    0.009,
                    paper,
                );
                d.line(
                    (0.61, 0.36 + i as f32 * 0.065),
                    (0.84, 0.31 + i as f32 * 0.065),
                    0.009,
                    paper,
                );
            }
            d.poly(
                &[
                    (0.70, 0.22),
                    (0.78, 0.20),
                    (0.78, 0.55),
                    (0.74, 0.51),
                    (0.70, 0.58),
                ],
                ink,
            );
        }
        (Layout::Objects, _) => {
            d.rect(0.475, 0.48, 0.05, 0.36, ink);
            d.ellipse(0.5, 0.85, 0.25, 0.047, b);
            d.poly(&[(0.28, 0.2), (0.71, 0.2), (0.86, 0.53), (0.13, 0.53)], a);
            d.ellipse(0.5, 0.53, 0.365, 0.045, b);
            d.circle(0.5, 0.57, 0.06, paper);
        }
        (Layout::Mosaic, 0) => {
            let points = [
                [(0.05, 0.1), (0.43, 0.05), (0.30, 0.44)],
                [(0.48, 0.07), (0.94, 0.17), (0.65, 0.48)],
                [(0.05, 0.16), (0.28, 0.5), (0.06, 0.85)],
                [(0.37, 0.39), (0.69, 0.53), (0.43, 0.92)],
                [(0.74, 0.39), (0.94, 0.23), (0.91, 0.94)],
                [(0.07, 0.91), (0.34, 0.61), (0.35, 0.93)],
                [(0.50, 0.93), (0.74, 0.59), (0.85, 0.94)],
            ];
            for (i, points) in points.iter().enumerate() {
                d.poly(
                    points,
                    if i % 3 == 0 {
                        ink
                    } else if i % 2 == 0 {
                        a
                    } else {
                        b
                    },
                );
            }
        }
        (Layout::Mosaic, 1) => {
            for y in 0..3 {
                for x in 0..3 {
                    let px = 0.06 + x as f32 * 0.3;
                    let py = 0.06 + y as f32 * 0.3;
                    let color = if (x + y) % 2 == 0 { a } else { b };
                    d.circle(px + 0.14, py + 0.14, 0.14, color);
                    if x % 2 == 0 {
                        d.rect(px, py, 0.14, 0.28, paper);
                    } else {
                        d.rect(px, py, 0.28, 0.14, paper);
                    }
                }
            }
        }
        (Layout::Mosaic, 2) => {
            for i in 0..9 {
                let inset = 0.04 + i as f32 * 0.05;
                let size = 0.92 - i as f32 * 0.10;
                d.rect(
                    inset,
                    inset,
                    size,
                    size,
                    if i % 3 == 0 {
                        a
                    } else if i % 3 == 1 {
                        b
                    } else {
                        paper
                    },
                );
            }
        }
        (Layout::Mosaic, _) => {
            for y in 0..9 {
                for x in 0..9 {
                    if (x * 3 + y * 5 + x * y) % 7 < 4 {
                        d.rect(
                            0.055 + x as f32 * 0.1,
                            0.055 + y as f32 * 0.1,
                            0.085,
                            0.085,
                            if (x + y) % 3 == 0 { b } else { a },
                        );
                    }
                }
            }
        }
        (Layout::Frame, 0) => {
            for &(x, y, sx, sy) in &[
                (0.07, 0.07, 1.0, 1.0),
                (0.93, 0.07, -1.0, 1.0),
                (0.07, 0.93, 1.0, -1.0),
                (0.93, 0.93, -1.0, -1.0),
            ] {
                d.line((x, y), (x + sx * 0.25, y), 0.035, a);
                d.line((x, y), (x, y + sy * 0.25), 0.035, a);
            }
            d.circle(0.5, 0.5, 0.22, b);
            d.star(0.5, 0.5, 0.14, 0.52, 6, paper);
        }
        (Layout::Frame, 1) => {
            for i in 0..6 {
                let n = 0.06 + i as f32 * 0.065;
                let r = 0.94 - i as f32 * 0.065;
                let color = if i % 2 == 0 { a } else { b };
                d.line((n, n), (r, n), 0.012, color);
                d.line((r, n), (r, r), 0.012, color);
                d.line((r, r), (n, r), 0.012, color);
                d.line((n, r), (n, n), 0.012, color);
            }
            d.circle(0.5, 0.5, 0.065, ink);
        }
        (Layout::Frame, 2) => {
            d.poly(&[(0.06, 0.17), (0.76, 0.04), (0.94, 0.81), (0.22, 0.96)], a);
            d.poly(
                &[(0.18, 0.26), (0.69, 0.15), (0.83, 0.73), (0.31, 0.85)],
                paper,
            );
            d.poly(&[(0.42, 0.40), (0.61, 0.35), (0.68, 0.61), (0.48, 0.66)], b);
        }
        (Layout::Frame, _) => {
            for i in 0..10 {
                let p = 0.08 + i as f32 * 0.093;
                d.circle(p, 0.08, 0.013, a);
                d.circle(p, 0.92, 0.013, a);
                if i > 0 && i < 9 {
                    d.circle(0.08, p, 0.013, a);
                    d.circle(0.92, p, 0.013, a);
                }
            }
            d.ring(0.5, 0.5, 0.29, 0.02, b);
            d.star(0.5, 0.5, 0.20, 0.42, 8, a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalogue_is_a_complete_original_year_with_unique_artwork() {
        assert_eq!(CATALOG.len(), 52);
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        let mut weeks = HashSet::new();
        let mut compositions = HashSet::new();
        for template in CATALOG {
            assert!(ids.insert(template.id));
            assert!(names.insert(template.name));
            assert!(weeks.insert(template.week));
            assert!(categories().contains(&template.category));
            assert!(!template.title.trim().is_empty());
            assert!(!template.body.trim().is_empty());
            assert!(!template.description.trim().is_empty());
            let document = build(template.id, 1080.0, 1080.0, 72.0).unwrap();
            // Ignore IDs and colours: each composition must differ in actual
            // geometry, so recolouring one drawing cannot pad the catalogue.
            let artwork = document.layers[1].kind.shapes().unwrap();
            assert!(!artwork.is_empty());
            let geometry: Vec<_> = artwork
                .iter()
                .map(|shape| {
                    (
                        &shape.geom,
                        shape.style.stroke.as_ref().map(|stroke| stroke.width),
                    )
                })
                .collect();
            assert!(
                compositions.insert(serde_json::to_string(&geometry).unwrap()),
                "duplicate composition {}",
                template.id
            );
        }
        assert!((1..=52).all(|week| weeks.contains(&week)));
    }

    #[test]
    fn every_template_fits_every_preset_and_remains_editable() {
        let mut sizes: Vec<_> = crate::presets::all()
            .iter()
            .map(|preset| (preset.w, preset.h, preset.dpi))
            .collect();
        sizes.extend([
            (16.0, 16.0, 72.0),
            (16.0, 1000.0, 72.0),
            (1000.0, 16.0, 72.0),
            (360.0, 2800.0, 144.0),
            (2800.0, 360.0, 96.0),
            (777.0, 1133.0, 120.0),
        ]);
        for template in CATALOG {
            for &(width, height, dpi) in &sizes {
                let document = build(template.id, width, height, dpi).unwrap();
                assert_eq!(
                    (document.width, document.height, document.dpi),
                    (width, height, dpi)
                );
                assert_eq!(document.artboards[0].size, Pt::new(width, height));
                let mut headlines = 0;
                for layer in &document.layers {
                    assert!(
                        layer.kind.pixels().is_none(),
                        "template allocated raster pixels"
                    );
                    for shape in layer.kind.shapes().unwrap() {
                        let bounds = shape.geom.bbox();
                        let bleed = shape
                            .style
                            .stroke
                            .as_ref()
                            .map_or(0.0, |stroke| stroke.width * 0.5);
                        assert!(
                            bounds.min.x - bleed >= -0.05
                                && bounds.min.y - bleed >= -0.05
                                && bounds.max.x + bleed <= width + 0.05
                                && bounds.max.y + bleed <= height + 0.05,
                            "{} at {width}×{height}: {} outside canvas: {:?}",
                            template.id,
                            shape.name,
                            bounds
                        );
                        if shape.name == "Headline" {
                            let Geom::Text(run) = &shape.geom else {
                                panic!("headline was outlined")
                            };
                            assert_eq!(run.content, template.title);
                            assert!(!run.contours.is_empty(), "headline is not drawable");
                            headlines += 1;
                        }
                    }
                }
                assert_eq!(headlines, 1);
            }
        }
    }

    #[test]
    fn templates_reflow_instead_of_stretching_and_validate_inputs() {
        let portrait = build("solar-social", 1080.0, 1920.0, 72.0).unwrap();
        let landscape = build("solar-social", 1920.0, 1080.0, 72.0).unwrap();
        let headline = |document: &Document| {
            document.layers[2]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .find(|shape| shape.name == "Headline")
                .unwrap()
                .geom
                .bbox()
        };
        assert!(headline(&portrait).min.y / portrait.height > 0.5);
        assert!(headline(&landscape).min.x / landscape.width >= 0.49);
        for (width, height, dpi) in [
            (f32::NAN, 100.0, 72.0),
            (100.0, f32::INFINITY, 72.0),
            (100.0, 100.0, 0.0),
            (0.0, 100.0, 72.0),
        ] {
            assert!(build("solar-social", width, height, dpi).is_err());
        }
        assert!(build("missing-template", 1080.0, 1080.0, 72.0).is_err());
        let restored: Document =
            serde_json::from_str(&serde_json::to_string(&portrait).unwrap()).unwrap();
        assert!(
            restored
                .layers
                .iter()
                .flat_map(|layer| layer.kind.shapes().unwrap())
                .any(|shape| matches!(&shape.geom,Geom::Text(run) if run.content=="SOLAR\nSOCIAL"))
        );
    }
}
