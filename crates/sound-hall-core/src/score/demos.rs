use crate::midi::event::MidiSong;

use super::parse_score;

/// Built-in musical demonstration entry.
#[derive(Clone, Copy, Debug)]
pub struct DemoSong {
    pub id: &'static str,
    pub title_en: &'static str,
    pub title_ja: &'static str,
    pub description: &'static str,
    pub score: &'static str,
}

const DEMO_ORCHESTRA: &str = r#"
tempo 104

track GrandPiano ch=0 program=0 vol=98 pan=54 rev=42
o4 l8 q7 v92
[c e g]4 g8 a8 g4 e4 | [d f a]4 f8 g8 f4 d4 |
[e g >c<]4 >c8 d8 c4< g4 | [d g b]2 [d g b]4 r4 |
[c e g]4 e8 g8 >c4< g4 | [f a >c<]4 a8 >c8 d4 c4 |
[g b >d<]4 b8 >d8 e4 d4 | [c e g >c<]1

track Violin ch=1 program=40 vol=92 pan=42 rev=48
o5 l4 q8 v88
g2 e8 f8 g4 | f2 d8 e8 f4 | e2 c8 d8 e4 | d2. r4 |
g4 e8 g8 >c4< g4 | a4 f8 a8 >d4 c4< | b4 g8 b8 >d4 f4 | e1

track Cello ch=2 program=42 vol=95 pan=82 rev=44
o3 l2 q8 v90
c2 e2 | d2 g2 | c2 e2 | g2. r4 |
c2 e2 | f2 d2 | g2 g2 | c1

track Flute ch=3 program=73 vol=82 pan=58 rev=48
o5 l4 q7 v80
r2 e8 f8 g4 | r2 d8 e8 f4 | r2 c8 d8 e4 | g2 d4 r4 |
e4 c8 e8 g4 e4 | f4 d8 f8 a4 f4 | g4 d8 g8 b4 g4 | >c1<

track FrenchHorn ch=4 program=60 vol=86 pan=74 rev=48
o3 l2 q8 v84
g2 g2 | a2 b2 | >c2< g2 | g2. r4 |
g2 >c2< | a2 a2 | g2 b2 | >c1<

track Harp ch=5 program=46 vol=78 pan=36 rev=55
o4 l8 q8 v78
c e g >c< g e c e | d f a >d< a f d f |
c e g >c< g e c e | g b >d g< d b g b |
c e g >c< g e c e | f a >c f< c a f a |
g b >d g< d b g b | [c e g >c<]1

track Timpani ch=6 program=47 vol=92 pan=64 rev=44
o3 l4 q7 v94
c4 r4 r2 | g4 r4 r2 | c4 r4 r2 | g4 g8 g8 g4 r4 |
c4 r4 r2 | f4 r4 d4 r4 | g4 r4 g4 r4 | c1
"#;

const DEMO_JAZZ: &str = r#"
tempo 126

track ElectricPiano ch=0 program=4 vol=95 pan=52 rev=34 cho=40
o4 l4 q7 v86
[d f a c]2 [g b d f]2 | [c e g b]2 [a c# e g]2 |
[d f a c]2 [g b d f]2 | [c e g b]1

track AcousticBass ch=1 program=32 vol=104 pan=64 rev=18
o2 l4 q7 v96
d4 f4 a4 f4 | g4 b4 >d4< b4 | >c4< b4 a4 g4 | a4 e4 a4 g4 |
d4 f4 a4 c4 | g4 b4 d4 g4 | c4 g4 e4 g4 | c1

track TenorSax ch=2 program=66 vol=98 pan=72 rev=38
o4 l8 q7 v92
r4 f8 a8 >c4< a8 f8 | g4 b8 >d8 f4 d4< | e4 g8 b8 >d4 c4< | a2. r4 |
d8 f8 a8 >c8 d4 c8< a8 | b4 g8 f8 d4 g4 | e8 g8 >c8 e8 g4 e4 | c1<

track Drums ch=9 vol=96 pan=64 rev=24
l8 v90
([36,42]8 42:8 [38,42]8 42:8 [36,42]8 42:8 [38,42]8 46:8)*3
[36,42,49]8 42:8 38:8 42:8 36:8 38:8 [36,49]4
"#;

const DEMO_WAGAKKI: &str = r#"
tempo 94

track Koto ch=0 program=107 vol=102 pan=44 rev=42
o4 l8 q8 v92
d8 e8 f8 a8 >d4< a8 f8 | e8 f8 a8 >d8 e4 d4< |
d8 f8 a8 >d8 f4 e8 d8 | a4 f8 e8 d2< |
(d8 f8 a8 >d8< a8 f8 e8 f8)*2 |
[d a >d<]1

track Shakuhachi ch=1 program=77 vol=98 pan=68 rev=50
o4 l4 q8 v90
r2 d4 e8 f8 | a2. f8 e8 | d2 f8 a8 >d4< | a2. r4 |
>d4.< e8 f4 e8 d8 | a4 f8 e8 d4 e8 f8 | a2 f8 e8 d4 | d1

track Shamisen ch=2 program=106 vol=94 pan=82 rev=30
o3 l8 q7 v94
d8 r8 d8 a8 >d8< r8 a8 d8 | f8 r8 f8 a8 >d8< r8 a8 f8 |
d8 r8 d8 a8 >d8 a8 f8 e8 | d4 a4 d2 |
d8 d8 a8 d8 f8 f8 a8 f8 | d8 d8 a8 d8 e8 f8 a8 e8 |
d1

track Taiko ch=3 program=116 vol=106 pan=64 rev=42
o2 l4 q7 v105
d4 r4 d8 d8 r4 | d4 r8 d8 d4 r4 | d4 r4 d8 d8 d8 d8 | d2 r2 |
d4 d8 d8 r4 d4 | d4 d8 d8 r4 d4 | d4 d8 d8 d4 d4 | d1

track Strings ch=4 program=49 vol=72 pan=64 rev=54
o3 l1 q8 v78
d1 | f1 | g1 | a1 |
d1 | f1 | a1 | d1
"#;

const DEMO_ELECTRONIC: &str = r#"
tempo 128

track SynthBass ch=0 program=38 vol=104 pan=64 rev=12
o2 l8 q6 v100
(a8 a8 >a8< a8 a8 a8 >g8< a8 | f8 f8 >f8< f8 f8 f8 >e8< f8 |
 g8 g8 >g8< g8 g8 g8 >f8< g8 | e8 e8 >e8< e8 e8 g8 e8)*2

track SuperSawLead ch=1 program=81 vol=88 pan=70 rev=38 cho=42
o5 l8 q7 v92
a4 e8 a8 >c4< b8 a8 | g4 d8 g8 b4 a8 g8 |
f4 c8 f8 a4 g8 f8 | e4 b8 >d8 e4 d4< |
a4 e8 a8 >c4 d8 e8 | f4 c8 f8 a4 g8 f8 |
g4 d8 g8 b4 >d4 | e1<

track WarmPad ch=2 program=89 vol=76 pan=58 rev=54 cho=50
o3 l1 q8 v78
[a >c e<]1 | [f a >c<]1 | [g b >d<]1 | [e g# b]1 |
[a >c e<]1 | [f a >c<]1 | [g b >d<]1 | [a >c e<]1

track CrystalFM ch=3 program=98 vol=74 pan=42 rev=55 cho=40
o5 l4 q8 v82
e4 a4 >c4 e4< | f4 a4 >c4 f4< | d4 g4 b4 >d4< | e2 b2 |
e4 a4 >c4 e4< | f4 a4 >c4 f4< | g4 b4 >d4 g4< | a1

track Drums ch=9 vol=100 pan=64 rev=24
l8 v96
([36,42]8 42:8 [38,42]8 42:8 [36,42]8 [36,42]8 [38,42]8 46:8)*7
[36,42,49]8 42:8 38:8 42:8 36:8 38:8 [36,57]4
"#;

const DEMO_GM_TOUR: &str = r#"
tempo 116

track Piano ch=0 program=0 vol=92 pan=50 rev=34
o4 l8 q7 v90
[c e g]4 >c8< g8 [c e g]2 | r1 | r1 | r1

track Vibraphone ch=1 program=11 vol=88 pan=76 rev=42
o5 l8 q8 v86
r1 | c8 e8 g8 >c8< b8 g8 e4 | r1 | r1

track AcousticGuitar ch=2 program=25 vol=92 pan=42 rev=32
o4 l8 q7 v88
r1 | r1 | [d f a]8 a8 [d f a]8 a8 [g b >d<]4 g4 | r1

track Trumpet ch=3 program=56 vol=90 pan=68 rev=36
o4 l8 q7 v90
r1 | r1 | r1 | g8 >c8 e8 g8 [c e g]2<

track Drums ch=9 vol=88 pan=64 rev=24
l8 v86
([36,42]8 42:8 [38,42]8 42:8 [36,42]8 42:8 [38,42]8 46:8)*3
[36,49]4 38:4 [36,57]2
"#;

const DEMO_MATSURI: &str = r#"
tempo 126

track Hyoshigi ch=0 program=拍子木 vol=94 pan=64 rev=40
o6 l4 q4 v118
c4 r4 c8 c8 r4 | r1 | r1 | r1 | r1 | r1 | r1 | r1 | r1 | c4 r4 c4 r4

track Shinobue ch=1 program=篠笛 vol=100 pan=58 rev=44
o5 l8 q7 v96
r1 |
d8 e8 g8 a8 b4 a8 g8 | a8 b8 >d8 e8 d4< b8 a8 | g8 a8 b8 a8 g8 e8 d8 e8 | g2 a4 r4 |
b8 >d8 e8 d8< b4 a8 g8 | a8 g8 e8 g8 a4 b8 a8 | g8 e8 d8 e8 g8 a8 g8 e8 | d2. r4 |
r1

track Shime ch=2 program=締太鼓 vol=84 pan=44 rev=30
o4 l8 q5 v100
r1 | (e8 e8 e16 e16 e8 e8 e8 e16 e16 e8)*8 | e4 r4 e4 r4

track Nagado ch=3 program=長胴太鼓 vol=94 pan=64 rev=36
o2 l4 q6 v110
g2 r2 | (g4 r8 g8 g8 g8 r4)*7 g8 g8 g8 g8 g4 g4 | g4 r4 g4 r4

track Atarigane ch=4 program=当り鉦 vol=78 pan=80 rev=30
o6 l8 q5 v92
r1 | (c8 c16 c16 c8 c16 c16 c8 c16 c16 c8 c16 c16)*8 | c4 r4 c4 r4

track Tsugaru ch=5 program=津軽三味線 vol=94 pan=36 rev=26
o4 l8 q6 v104
r1 | (d8 r8 a8 d8 >d8< a8 g8 a8 | d8 r8 a8 >d8 e8 d8< b8 a8)*4 | d4 r4 d4 r4
"#;

const DEMO_GAGAKU: &str = r#"
tempo 56

track Sho ch=0 program=笙 vol=82 pan=64 rev=62
o5 l1 q8 v80
[e f# a b]1 | [e a b >e<]1 | [d# e f# b]1 | [e f# a b]1 |
[f# a b >e<]1 | [e f# a b]1 | [d# e a b]1 | [e f# a b >e<]1

track Hichiriki ch=1 program=篳篥 vol=100 pan=56 rev=54
o4 l4 q8 v94
e2 f#4 a4 | b2. a4 | f#2 e4 f#4 | e1 |
a2 b4 >c#4< | b2 a4 f#4 | e2 f#4 a4 | e1

track Ryuteki ch=2 program=龍笛 vol=86 pan=72 rev=58
o5 l4 q8 v86
e2 f#8 e8 a4 | b2. a8 b8 | f#2 e4 f#4 | e1 |
a2 b8 a8 >c#4< | b2 a4 f#4 | e2 f#8 e8 a4 | e1

track Gakuso ch=3 program=楽箏 vol=90 pan=40 rev=52
o3 l4 q8 v84
([e b]4 >e4 f#8 e8< b4 | [a >e<]4 >a4 b8 a8 e4<)*4

track Kakko ch=4 program=締太鼓 vol=64 pan=28 rev=48
o4 l16 q5 v62
(r1 | r2 e16 e16 e16 e16 e16 e16 e16 e16)*4

track Taiko ch=5 program=長胴太鼓 vol=96 pan=64 rev=60
o2 l4 q6 v100
r1 | r1 | r1 | v70 g4 r4 v112 g2 | r1 | r1 | r1 | v70 g4 r4 v116 g2
"#;

pub static DEMO_SONGS: [DemoSong; 7] = [
    DemoSong {
        id: "orchestra",
        title_en: "Symphonic Dawn",
        title_ja: "交響的夜明け (オーケストラ)",
        description: "Grand Piano, Violin, Cello, Flute, French Horn, Harp & Timpani",
        score: DEMO_ORCHESTRA,
    },
    DemoSong {
        id: "jazz",
        title_en: "Midnight Quartet",
        title_ja: "真夜中のカルテット (ジャズ)",
        description: "Electric Piano, Acoustic Bass, Tenor Sax & Jazz Drum Kit",
        score: DEMO_JAZZ,
    },
    DemoSong {
        id: "wagakki",
        title_en: "Sakura & Bamboo",
        title_ja: "桜と竹 (和楽器アンサンブル)",
        description: "Koto, Shakuhachi, Shamisen, Taiko Drum & Slow Strings",
        score: DEMO_WAGAKKI,
    },
    DemoSong {
        id: "matsuri",
        title_en: "Festival Drums",
        title_ja: "祭囃子 (和楽器バンク)",
        description: "Shinobue, Shime & Nagado Taiko, Atarigane, Tsugaru Shamisen & Hyoshigi",
        score: DEMO_MATSURI,
    },
    DemoSong {
        id: "gagaku",
        title_en: "Court Music",
        title_ja: "雅楽 (和楽器バンク)",
        description: "Sho, Hichiriki, Ryuteki, Gakuso, Kakko & Taiko",
        score: DEMO_GAGAKU,
    },
    DemoSong {
        id: "electronic",
        title_en: "Neon Horizon",
        title_ja: "ネオン・ホライズン (エレクトロニック)",
        description: "Synth Bass, SuperSaw Lead, Warm Pad, Crystal FM & Electronic Drums",
        score: DEMO_ELECTRONIC,
    },
    DemoSong {
        id: "gm-tour",
        title_en: "General MIDI Showcase",
        title_ja: "General MIDI 楽器ショーケース",
        description: "Grand Piano, Vibraphone, Steel Guitar, Trumpet & Drum Kit",
        score: DEMO_GM_TOUR,
    },
];

pub fn list_demos() -> &'static [DemoSong] {
    &DEMO_SONGS
}

pub fn get_demo(id: &str) -> Option<&'static DemoSong> {
    let q = id.trim().to_ascii_lowercase();
    DEMO_SONGS.iter().find(|d| d.id == q)
}

pub fn parse_demo(id: &str) -> Option<MidiSong> {
    let demo = get_demo(id)?;
    parse_score(demo.score).ok()
}
