import { SoundHallWasm } from "./sound-hall.js";

/** Complete 128 General MIDI Melodic Preset Catalog */
const GM_PRESETS = [
  // 0-7 Piano
  [0, "Acoustic Grand Piano", "アコースティック・グランドピアノ", "Piano", "HammeredString", 30, 5],
  [1, "Bright Acoustic Piano", "ブライト・アコースティックピアノ", "Piano", "HammeredString", 28, 6],
  [2, "Electric Grand Piano", "エレクトリック・グランドピアノ", "Piano", "HammeredString", 26, 38],
  [3, "Honky-Tonk Piano", "ホンキートンク・ピアノ", "Piano", "HammeredString", 24, 12],
  [4, "Electric Piano 1", "エレクトリックピアノ1 (ローズ)", "Piano", "Fm4Op", 28, 42],
  [5, "Electric Piano 2", "エレクトリックピアノ2 (FM)", "Piano", "Fm4Op", 32, 48],
  [6, "Harpsichord", "ハープシコード (チェンバロ)", "Piano", "PluckedString", 30, 0],
  [7, "Clavinet", "クラビネット", "Piano", "PluckedString", 15, 8],
  // 8-15 Chromatic Percussion
  [8, "Celesta", "チェレスタ", "Chromatic Percussion", "ModalPercussion", 38, 10],
  [9, "Glockenspiel", "グロッケンシュピール", "Chromatic Percussion", "ModalPercussion", 40, 0],
  [10, "Music Box", "オルゴール", "Chromatic Percussion", "ModalPercussion", 35, 0],
  [11, "Vibraphone", "ヴィブラフォン", "Chromatic Percussion", "ModalPercussion", 34, 8],
  [12, "Marimba", "マリンバ", "Chromatic Percussion", "ModalPercussion", 28, 0],
  [13, "Xylophone", "シロフォン (木琴)", "Chromatic Percussion", "ModalPercussion", 25, 0],
  [14, "Tubular Bells", "チューブラーベル", "Chromatic Percussion", "ModalPercussion", 50, 0],
  [15, "Dulcimer", "ダルシマー", "Chromatic Percussion", "PluckedString", 32, 10],
  // 16-23 Organ
  [16, "Drawbar Organ", "ドローバー・オルガン", "Organ", "AdditiveOrgan", 26, 25],
  [17, "Percussive Organ", "パーカッシブ・オルガン", "Organ", "AdditiveOrgan", 24, 18],
  [18, "Rock Organ", "ロック・オルガン", "Organ", "AdditiveOrgan", 26, 20],
  [19, "Church Organ", "チャーチ・オルガン (パイプオルガン)", "Organ", "AdditiveOrgan", 62, 15],
  [20, "Reed Organ", "リード・オルガン", "Organ", "AdditiveOrgan", 30, 12],
  [21, "Accordion", "アコーディオン", "Organ", "AdditiveOrgan", 24, 45],
  [22, "Harmonica", "ハーモニカ", "Organ", "AdditiveOrgan", 26, 10],
  [23, "Tango Accordion", "タンゴ・アコーディオン (バンドネオン)", "Organ", "AdditiveOrgan", 26, 22],
  // 24-31 Guitar
  [24, "Acoustic Guitar (Nylon)", "アコースティックギター (ナイロン弦)", "Guitar", "PluckedString", 28, 0],
  [25, "Acoustic Guitar (Steel)", "アコースティックギター (スティール弦)", "Guitar", "PluckedString", 28, 10],
  [26, "Electric Guitar (Jazz)", "エレクトリックギター (ジャズ)", "Guitar", "PluckedString", 26, 12],
  [27, "Electric Guitar (Clean)", "エレクトリックギター (クリーン)", "Guitar", "PluckedString", 30, 42],
  [28, "Electric Guitar (Muted)", "エレクトリックギター (ミュート)", "Guitar", "PluckedString", 18, 10],
  [29, "Overdriven Guitar", "オーバードライブ・ギター", "Guitar", "PluckedString", 28, 12],
  [30, "Distortion Guitar", "ディストーション・ギター", "Guitar", "PluckedString", 28, 15],
  [31, "Guitar Harmonics", "ギター・ハーモニクス", "Guitar", "PluckedString", 36, 25],
  // 32-39 Bass
  [32, "Acoustic Bass", "アコースティック・ベース", "Bass", "PluckedString", 20, 0],
  [33, "Electric Bass (Finger)", "エレクトリック・ベース (フィンガー)", "Bass", "PluckedString", 14, 5],
  [34, "Electric Bass (Pick)", "エレクトリック・ベース (ピック)", "Bass", "PluckedString", 14, 5],
  [35, "Fretless Bass", "フレットレス・ベース", "Bass", "FormantSubtractive", 20, 25],
  [36, "Slap Bass 1", "スラップ・ベース1", "Bass", "Fm4Op", 14, 5],
  [37, "Slap Bass 2", "スラップ・ベース2", "Bass", "Fm4Op", 16, 8],
  [38, "Synth Bass 1", "シンセベース1", "Bass", "FormantSubtractive", 12, 10],
  [39, "Synth Bass 2", "シンセベース2", "Bass", "FormantSubtractive", 12, 15],
  // 40-47 Strings
  [40, "Violin", "ヴァイオリン", "Strings", "BowedString", 36, 6],
  [41, "Viola", "ヴィオラ", "Strings", "BowedString", 36, 6],
  [42, "Cello", "チェロ", "Strings", "BowedString", 36, 5],
  [43, "Contrabass", "コントラバス", "Strings", "BowedString", 32, 0],
  [44, "Tremolo Strings", "トレモロ・ストリングス", "Strings", "BowedString", 40, 22],
  [45, "Pizzicato Strings", "ピッツィカート・ストリングス", "Strings", "PluckedString", 30, 14],
  [46, "Orchestral Harp", "オーケストラル・ハープ", "Strings", "PluckedString", 42, 6],
  [47, "Timpani", "ティンパニ", "Strings", "ModalPercussion", 38, 0],
  // 48-55 Ensemble
  [48, "String Ensemble 1", "ストリングス・アンサンブル1", "Ensemble", "BowedString", 44, 28],
  [49, "String Ensemble 2", "ストリングス・アンサンブル2 (スロー)", "Ensemble", "BowedString", 48, 32],
  [50, "Synth Strings 1", "シンセストリングス1", "Ensemble", "FormantSubtractive", 38, 48],
  [51, "Synth Strings 2", "シンセストリングス2", "Ensemble", "FormantSubtractive", 42, 52],
  [52, "Choir Aahs", "クワイア・アー (合唱)", "Ensemble", "FormantSubtractive", 52, 35],
  [53, "Voice Oohs", "ボイス・ウー", "Ensemble", "FormantSubtractive", 48, 30],
  [54, "Synth Voice", "シンセボイス", "Ensemble", "FormantSubtractive", 48, 40],
  [55, "Orchestra Hit", "オーケストラ・ヒット", "Ensemble", "FormantSubtractive", 42, 25],
  // 56-63 Brass
  [56, "Trumpet", "トランペット", "Brass", "WindBore", 30, 0],
  [57, "Trombone", "トロンボーン", "Brass", "WindBore", 30, 0],
  [58, "Tuba", "チューバ", "Brass", "WindBore", 28, 0],
  [59, "Muted Trumpet", "ミュート・トランペット", "Brass", "WindBore", 26, 0],
  [60, "French Horn", "フレンチ・ホルン", "Brass", "WindBore", 40, 10],
  [61, "Brass Section", "ブラス・セクション", "Brass", "FormantSubtractive", 34, 24],
  [62, "Synth Brass 1", "シンセブラス1", "Brass", "FormantSubtractive", 28, 35],
  [63, "Synth Brass 2", "シンセブラス2", "Brass", "FormantSubtractive", 28, 42],
  // 64-71 Reed
  [64, "Soprano Sax", "ソプラノ・サックス", "Reed", "WindBore", 30, 6],
  [65, "Alto Sax", "アルト・サックス", "Reed", "WindBore", 30, 8],
  [66, "Tenor Sax", "テナー・サックス", "Reed", "WindBore", 32, 8],
  [67, "Baritone Sax", "バリトン・サックス", "Reed", "WindBore", 26, 0],
  [68, "Oboe", "オーボエ", "Reed", "WindBore", 34, 0],
  [69, "English Horn", "イングリッシュ・ホルン", "Reed", "WindBore", 36, 0],
  [70, "Bassoon", "ファゴット (バスーン)", "Reed", "WindBore", 32, 0],
  [71, "Clarinet", "クラリネット", "Reed", "WindBore", 32, 4],
  // 72-79 Pipe
  [72, "Piccolo", "ピッコロ", "Pipe", "WindBore", 36, 0],
  [73, "Flute", "フルート", "Pipe", "WindBore", 36, 5],
  [74, "Recorder", "リコーダー", "Pipe", "WindBore", 34, 0],
  [75, "Pan Flute", "パンフルート", "Pipe", "WindBore", 44, 8],
  [76, "Blown Bottle", "ブロウン・ボトル (瓶笛)", "Pipe", "WindBore", 42, 6],
  [77, "Shakuhachi", "尺八", "Pipe", "WindBore", 44, 0],
  [78, "Whistle", "口笛 (ホイッスル)", "Pipe", "WindBore", 34, 0],
  [79, "Ocarina", "オカリナ", "Pipe", "WindBore", 38, 0],
  // 80-87 Synth Lead
  [80, "Lead 1 (Square)", "リード1 (スクエア)", "Synth Lead", "FormantSubtractive", 26, 22],
  [81, "Lead 2 (Sawtooth)", "リード2 (ソウ)", "Synth Lead", "FormantSubtractive", 28, 30],
  [82, "Lead 3 (Calliope)", "リード3 (カリオペ)", "Synth Lead", "FormantSubtractive", 34, 28],
  [83, "Lead 4 (Chiff)", "リード4 (チフ)", "Synth Lead", "Fm4Op", 34, 25],
  [84, "Lead 5 (Charang)", "リード5 (チャラン)", "Synth Lead", "FormantSubtractive", 28, 28],
  [85, "Lead 6 (Voice)", "リード6 (ボイス)", "Synth Lead", "FormantSubtractive", 40, 35],
  [86, "Lead 7 (Fifths)", "リード7 (フィフス)", "Synth Lead", "FormantSubtractive", 34, 30],
  [87, "Lead 8 (Bass + Lead)", "リード8 (ベース+リード)", "Synth Lead", "FormantSubtractive", 22, 20],
  // 88-95 Synth Pad
  [88, "Pad 1 (New Age)", "パッド1 (ニューエイジ)", "Synth Pad", "Fm4Op", 52, 42],
  [89, "Pad 2 (Warm)", "パッド2 (ウォーム)", "Synth Pad", "FormantSubtractive", 48, 48],
  [90, "Pad 3 (Polysynth)", "パッド3 (ポリシンセ)", "Synth Pad", "FormantSubtractive", 44, 52],
  [91, "Pad 4 (Choir)", "パッド4 (クワイア)", "Synth Pad", "FormantSubtractive", 58, 45],
  [92, "Pad 5 (Bowed)", "パッド5 (ボウイング・グラス)", "Synth Pad", "Fm4Op", 56, 32],
  [93, "Pad 6 (Metallic)", "パッド6 (メタリック)", "Synth Pad", "Fm4Op", 54, 38],
  [94, "Pad 7 (Halo)", "パッド7 (ハロー)", "Synth Pad", "FormantSubtractive", 62, 55],
  [95, "Pad 8 (Sweep)", "パッド8 (スウィープ)", "Synth Pad", "FormantSubtractive", 52, 45],
  // 96-103 Synth Effects
  [96, "FX 1 (Rain)", "FX 1 (レイン)", "Synth Effects", "Fm4Op", 60, 45],
  [97, "FX 2 (Soundtrack)", "FX 2 (サウンドトラック)", "Synth Effects", "FormantSubtractive", 58, 50],
  [98, "FX 3 (Crystal)", "FX 3 (クリスタル)", "Synth Effects", "Fm4Op", 54, 38],
  [99, "FX 4 (Atmosphere)", "FX 4 (アトモスフィア)", "Synth Effects", "Fm4Op", 58, 45],
  [100, "FX 5 (Brightness)", "FX 5 (ブライトネス)", "Synth Effects", "Fm4Op", 52, 42],
  [101, "FX 6 (Goblins)", "FX 6 (ゴブリン)", "Synth Effects", "FormantSubtractive", 56, 48],
  [102, "FX 7 (Echoes)", "FX 7 (エコー)", "Synth Effects", "FormantSubtractive", 68, 52],
  [103, "FX 8 (Sci-Fi)", "FX 8 (サイファイ / SF)", "Synth Effects", "FormantSubtractive", 52, 40],
  // 104-111 Ethnic
  [104, "Sitar", "シタール", "Ethnic", "PluckedString", 36, 8],
  [105, "Banjo", "バンジョー", "Ethnic", "PluckedString", 24, 0],
  [106, "Shamisen", "三味線", "Ethnic", "PluckedString", 26, 0],
  [107, "Koto", "箏 (琴)", "Ethnic", "PluckedString", 36, 0],
  [108, "Kalimba", "カリンバ (親指ピアノ)", "Ethnic", "ModalPercussion", 32, 0],
  [109, "Bagpipe", "バグパイプ", "Ethnic", "WindBore", 38, 8],
  [110, "Fiddle", "フィドル", "Ethnic", "BowedString", 30, 0],
  [111, "Shanai", "シャナイ (チャルメラ)", "Ethnic", "WindBore", 34, 0],
  // 112-119 Percussive
  [112, "Tinkle Bell", "ティンクルベル", "Percussive", "ModalPercussion", 42, 0],
  [113, "Agogo", "アゴゴ", "Percussive", "ModalPercussion", 25, 0],
  [114, "Steel Drums", "スティールドラム (スティールパン)", "Percussive", "ModalPercussion", 34, 8],
  [115, "Woodblock", "ウッドブロック", "Percussive", "ModalPercussion", 20, 0],
  [116, "Taiko Drum", "和太鼓 (太鼓)", "Percussive", "ModalPercussion", 36, 0],
  [117, "Melodic Tom", "メロディック・タム", "Percussive", "FormantSubtractive", 28, 0],
  [118, "Synth Drum", "シンセ・ドラム", "Percussive", "FormantSubtractive", 24, 0],
  [119, "Reverse Cymbal", "リバース・シンバル", "Percussive", "FormantSubtractive", 45, 25],
  // 120-127 Sound Effects
  [120, "Guitar Fret Noise", "ギター・フレットノイズ", "Sound Effects", "FormantSubtractive", 22, 0],
  [121, "Breath Noise", "ブレスノイズ", "Sound Effects", "FormantSubtractive", 35, 10],
  [122, "Seashore", "波の音 (シーショア)", "Sound Effects", "FormantSubtractive", 50, 30],
  [123, "Bird Tweet", "鳥のさえずり", "Sound Effects", "FormantSubtractive", 42, 0],
  [124, "Telephone Ring", "電話のベル", "Sound Effects", "Fm4Op", 28, 0],
  [125, "Helicopter", "ヘリコプター", "Sound Effects", "FormantSubtractive", 25, 15],
  [126, "Applause", "拍手 (アプローズ)", "Sound Effects", "FormantSubtractive", 54, 40],
  [127, "Gunshot", "銃声 (ガンショット)", "Sound Effects", "FormantSubtractive", 46, 0],
];

/** SoundHall 和楽器 Bank (Bank Select MSB 1): [prog, en, ja, family, engine, rev, cho, previewNote] */
const WAGAKKI_BANK = 1;
const WAGAKKI_PRESETS = [
  [0, "Biwa", "琵琶", "Ethnic", "PluckedString", 38, 0, 52],
  [1, "Jushichigen (Bass Koto)", "十七絃", "Ethnic", "PluckedString", 40, 0, 43],
  [2, "Tsugaru Shamisen", "津軽三味線", "Ethnic", "PluckedString", 28, 0, 57],
  [3, "Taishogoto", "大正琴", "Ethnic", "PluckedString", 34, 0, 72],
  [4, "Gakuso (Gagaku Koto)", "楽箏", "Ethnic", "PluckedString", 42, 0, 64],
  [5, "Kokyu", "胡弓", "Strings", "BowedString", 40, 0, 69],
  [6, "Shinobue", "篠笛", "Pipe", "WindBore", 40, 0, 79],
  [7, "Ryuteki", "龍笛", "Pipe", "WindBore", 46, 0, 76],
  [8, "Nokan", "能管", "Pipe", "WindBore", 42, 0, 81],
  [9, "Hichiriki", "篳篥", "Reed", "WindBore", 44, 0, 69],
  [10, "Sho", "笙", "Organ", "AdditiveOrgan", 50, 0, 76],
  [11, "Horagai (Conch)", "法螺貝", "Brass", "WindBore", 46, 0, 50],
  [12, "Shakuhachi (Muraiki)", "尺八 (ムラ息)", "Pipe", "WindBore", 46, 0, 62],
  [13, "Nagado-daiko", "長胴太鼓", "Percussive", "ModalPercussion", 34, 0, 38],
  [14, "Shime-daiko", "締太鼓", "Percussive", "ModalPercussion", 28, 0, 64],
  [15, "O-daiko", "大太鼓", "Percussive", "ModalPercussion", 40, 0, 31],
  [16, "Kotsuzumi", "小鼓", "Percussive", "ModalPercussion", 36, 0, 67],
  [17, "Otsuzumi", "大鼓", "Percussive", "ModalPercussion", 30, 0, 72],
  [18, "Atarigane", "当り鉦", "Chromatic Percussion", "ModalPercussion", 30, 0, 84],
  [19, "Hyoshigi", "拍子木", "Percussive", "ModalPercussion", 32, 0, 84],
  [20, "Mokugyo", "木魚", "Percussive", "ModalPercussion", 34, 0, 60],
  [21, "Rin (Singing Bowl)", "りん", "Chromatic Percussion", "ModalPercussion", 50, 0, 79],
  [22, "Suzu (Shrine Bells)", "鈴", "Chromatic Percussion", "ModalPercussion", 38, 0, 88],
];

/** Unified instrument catalog across banks (0 = General MIDI, 1 = 和楽器). */
const CATALOG = [
  ...GM_PRESETS.map(([prog, en, ja, fam, eng, rev, cho]) => ({
    bank: 0, prog, en, ja, fam, eng, rev, cho, preview: 60,
  })),
  ...WAGAKKI_PRESETS.map(([prog, en, ja, fam, eng, rev, cho, preview]) => ({
    bank: WAGAKKI_BANK, prog, en, ja, fam, eng, rev, cho, preview,
  })),
];

function findPreset(bank, prog) {
  return (
    CATALOG.find((p) => p.bank === bank && p.prog === prog) ??
    CATALOG.find((p) => p.bank === 0 && p.prog === (prog & 0x7f))
  );
}

function presetLabel(p) {
  return p.bank === 0
    ? `#${String(p.prog).padStart(3, "0")}`
    : `和${String(p.prog).padStart(2, "0")}`;
}

/** Complete 47 General MIDI Standard Drum Kit Voices (Notes 35..81) */
const GM_DRUMS = [
  [35, "Acoustic Bass Drum", "アコースティック・バスドラム", ""],
  [36, "Bass Drum 1", "バスドラム1", ""],
  [37, "Side Stick", "サイドスティック (リムショット)", ""],
  [38, "Acoustic Snare", "アコースティック・スネア", ""],
  [39, "Hand Clap", "ハンドクラップ", ""],
  [40, "Electric Snare", "エレクトリック・スネア", ""],
  [41, "Low Floor Tom", "ロー・フロアタム", ""],
  [42, "Closed Hi-Hat", "クローズド・ハイハット", "Choke G1"],
  [43, "High Floor Tom", "ハイ・フロアタム", ""],
  [44, "Pedal Hi-Hat", "ペダル・ハイハット", "Choke G1"],
  [45, "Low Tom", "ロー・タム", ""],
  [46, "Open Hi-Hat", "オープン・ハイハット", "Choke G1"],
  [47, "Low-Mid Tom", "ローミッド・タム", ""],
  [48, "Hi-Mid Tom", "ハイミッド・タム", ""],
  [49, "Crash Cymbal 1", "クラッシュシンバル1", ""],
  [50, "High Tom", "ハイ・タム", ""],
  [51, "Ride Cymbal 1", "ライドシンバル1", ""],
  [52, "Chinese Cymbal", "チャイニーズシンバル", ""],
  [53, "Ride Bell", "ライドベル", ""],
  [54, "Tambourine", "タンバリン", ""],
  [55, "Splash Cymbal", "スプラッシュシンバル", ""],
  [56, "Cowbell", "カウベル", ""],
  [57, "Crash Cymbal 2", "クラッシュシンバル2", ""],
  [58, "Vibraslap", "ヴィブラスラップ", ""],
  [59, "Ride Cymbal 2", "ライドシンバル2", ""],
  [60, "Hi Bongo", "ハイ・ボンゴ", ""],
  [61, "Low Bongo", "ロー・ボンゴ", ""],
  [62, "Mute Hi Conga", "ミュート・ハイコンガ", ""],
  [63, "Open Hi Conga", "オープン・ハイコンガ", ""],
  [64, "Low Conga", "ロー・コンガ", ""],
  [65, "High Timbale", "ハイ・ティンバレス", ""],
  [66, "Low Timbale", "ロー・ティンバレス", ""],
  [67, "High Agogo", "ハイ・アゴゴ", ""],
  [68, "Low Agogo", "ロー・アゴゴ", ""],
  [69, "Cabasa", "カバサ", ""],
  [70, "Maracas", "マラカス", ""],
  [71, "Short Whistle", "ショート・ホイッスル", "Choke G2"],
  [72, "Long Whistle", "ロング・ホイッスル", "Choke G2"],
  [73, "Short Guiro", "ショート・ギロ", "Choke G3"],
  [74, "Long Guiro", "ロング・ギロ", "Choke G3"],
  [75, "Claves", "クラベス", ""],
  [76, "Hi Wood Block", "ハイ・ウッドブロック", ""],
  [77, "Low Wood Block", "ロー・ウッドブロック", ""],
  [78, "Mute Cuica", "ミュート・クイーカ", ""],
  [79, "Open Cuica", "オープン・クイーカ", ""],
  [80, "Mute Triangle", "ミュート・トライアングル", "Choke G4"],
  [81, "Open Triangle", "オープン・トライアングル", "Choke G4"],
];

const DEMO_SCORES = [
  // 0: Orchestra
  `# Symphonic Dawn (交響的夜明け)
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
c4 r4 r2 | f4 r4 d4 r4 | g4 r4 g4 r4 | c1`,

  // 1: Jazz
  `# Midnight Quartet (真夜中のカルテット)
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
[36,42,49]8 42:8 38:8 42:8 36:8 38:8 [36,49]4`,

  // 2: Wagakki
  `# Sakura & Bamboo (桜と竹 — 和楽器アンサンブル)
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
d1 | f1 | a1 | d1`,

  // 3: Electronic
  `# Neon Horizon (ネオン・ホライズン)
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
[36,42,49]8 42:8 38:8 42:8 36:8 38:8 [36,57]4`,

  // 4: GM Tour
  `# General MIDI Showcase (GM 楽器ツアー)
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
[36,49]4 38:4 [36,57]2`,
  // 5: Matsuri (和楽器バンク)
  `# Festival Drums (祭囃子・和楽器バンク)
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
r1 | (d8 r8 a8 d8 >d8< a8 g8 a8 | d8 r8 a8 >d8 e8 d8< b8 a8)*4 | d4 r4 d4 r4`,
  // 6: Gagaku (和楽器バンク)
  `# Court Music (雅楽・和楽器バンク)
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
r1 | r1 | r1 | v70 g4 r4 v112 g2 | r1 | r1 | r1 | v70 g4 r4 v116 g2`,
];

const WAGAKKI_FAMILY = "和楽器";
const FAMILIES = [
  "All",
  WAGAKKI_FAMILY,
  "Piano",
  "Chromatic Percussion",
  "Organ",
  "Guitar",
  "Bass",
  "Strings",
  "Ensemble",
  "Brass",
  "Reed",
  "Pipe",
  "Synth Lead",
  "Synth Pad",
  "Synth Effects",
  "Ethnic",
  "Percussive",
  "Sound Effects",
];

const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

function midiToNoteName(note) {
  const n = Math.max(0, Math.min(127, note | 0));
  const oct = Math.floor(n / 12) - 1;
  return `${NOTE_NAMES[n % 12]}${oct}`;
}

function formatTime(sec) {
  const s = Math.max(0, Number.isFinite(sec) ? sec : 0);
  const mins = Math.floor(s / 60);
  const rem = (s % 60).toFixed(1).padStart(4, "0");
  return `${mins}:${rem}`;
}

/** Syncs custom CSS variables for range slider track progress & aria-valuetext */
function syncSliderCss(slider, ariaText) {
  slider.style.setProperty("--attr-min", slider.min || 0);
  slider.style.setProperty("--attr-max", slider.max || 100);
  slider.style.setProperty("--control-value", slider.value);
  if (ariaText) {
    slider.setAttribute("aria-valuetext", ariaText);
  }
}

class SoundHallStudioApp {
  constructor() {
    this.wasmBytes = null;
    this.synth = null;
    this.audioCtx = null;
    this.analyser = null;
    this.scriptNode = null;

    this.selectedBank = 0;
    this.selectedProgram = 0;
    this.selectedFamily = "All";
    this.searchQuery = "";
    this.baseOctave = 3; // C3 (48) to B5 (83)
    this.keyVelocity = 100;
    this.sustainOn = false;
    this.userHeldNotes = new Set();
    this.kbHeldNotes = new Map();
    this.pointerActiveNote = null;
    this.isTimelineScrubbing = false;
    this.spectrumPeaks = new Float32Array(64);

    this.cacheDom();
    this.initSliders();
    this.renderFamilyChips();
    this.renderInstrumentList();
    this.renderChannelMatrix();
    this.renderPianoKeyboard();
    this.renderDrumPads();
    this.bindEvents();
    this.preloadWasm();
    this.initWebMidi();
    requestAnimationFrame(() => this.animationLoop());
  }

  cacheDom() {
    this.elStatusEngine = document.getElementById("status-engine");
    this.elStatusVoices = document.getElementById("status-voices");
    this.elStatusPatch = document.getElementById("status-patch");
    this.elStatusMidi = document.getElementById("status-midi");
    this.elBtnPanic = document.getElementById("btn-panic");

    this.canvasScope = document.getElementById("canvas-scope");
    this.ctxScope = this.canvasScope.getContext("2d");
    this.elReadoutPeak = document.getElementById("readout-peak");

    this.canvasSpectrum = document.getElementById("canvas-spectrum");
    this.ctxSpectrum = this.canvasSpectrum.getContext("2d");
    this.elReadoutEngineKind = document.getElementById("readout-engine-kind");

    this.elBtnAuditionNote = document.getElementById("btn-audition-note");
    this.elBtnAuditionChord = document.getElementById("btn-audition-chord");
    this.elBtnSustain = document.getElementById("btn-sustain");

    this.sliderVolume = document.getElementById("slider-volume");
    this.valVolume = document.getElementById("val-volume");
    this.sliderReverb = document.getElementById("slider-reverb");
    this.valReverb = document.getElementById("val-reverb");
    this.sliderChorus = document.getElementById("slider-chorus");
    this.valChorus = document.getElementById("val-chorus");
    this.sliderVelocity = document.getElementById("slider-velocity");
    this.valVelocity = document.getElementById("val-velocity");
    this.sliderMod = document.getElementById("slider-mod");
    this.valMod = document.getElementById("val-mod");
    this.sliderBend = document.getElementById("slider-bend");
    this.valBend = document.getElementById("val-bend");

    this.chkAutoAudition = document.getElementById("chk-auto-audition");
    this.inputSearch = document.getElementById("instrument-search");
    this.elInstrumentCount = document.getElementById("instrument-count");
    this.elFamilyChips = document.getElementById("family-chips");
    this.elInstrumentList = document.getElementById("instrument-list");

    this.demoButtons = Array.from(document.querySelectorAll(".demo-btn"));
    this.elBtnSeqPlay = document.getElementById("btn-seq-play");
    this.elBtnSeqStop = document.getElementById("btn-seq-stop");
    this.sliderTimeline = document.getElementById("slider-timeline");
    this.valTimeline = document.getElementById("val-timeline");
    this.inputFileImport = document.getElementById("file-import");
    this.elBtnExportWav = document.getElementById("btn-export-wav");

    this.elChannelMatrix = document.getElementById("channel-matrix");
    this.elScoreStatus = document.getElementById("score-status");
    this.elBtnApplyScore = document.getElementById("btn-apply-score");
    this.elScoreTextarea = document.getElementById("score-textarea");

    this.elTabPiano = document.getElementById("tab-piano");
    this.elTabDrums = document.getElementById("tab-drums");
    this.elPianoToolbar = document.getElementById("piano-toolbar");
    this.elBtnOctDown = document.getElementById("btn-oct-down");
    this.elBtnOctUp = document.getElementById("btn-oct-up");
    this.elLabelOctaveRange = document.getElementById("label-octave-range");
    this.elViewPiano = document.getElementById("view-piano");
    this.elPianoKeyboard = document.getElementById("piano-keyboard");
    this.elViewDrums = document.getElementById("view-drums");
    this.elDrumPadsGrid = document.getElementById("drum-pads-grid");
  }

  initSliders() {
    for (const s of [
      this.sliderVolume,
      this.sliderReverb,
      this.sliderChorus,
      this.sliderVelocity,
      this.sliderMod,
      this.sliderBend,
      this.sliderTimeline,
    ]) {
      syncSliderCss(s);
    }
    this.elScoreTextarea.value = DEMO_SCORES[0];
  }

  async preloadWasm() {
    try {
      const res = await fetch("./sound_hall_wasm.wasm");
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      this.wasmBytes = await res.arrayBuffer();
      // Instantiate an offline-capable WASM studio @ 44100 Hz immediately for score validation/WAV export
      this.synth = await SoundHallWasm.instantiate(this.wasmBytes, 44100, 64);
      this.synth.loadDemo(0);
      this.updateTimelineReadout();
    } catch (err) {
      this.elStatusEngine.textContent = `WASM Error: ${err.message}`;
    }
  }

  async ensureAudioStarted() {
    if (this.audioCtx) {
      if (this.audioCtx.state === "suspended") {
        await this.audioCtx.resume();
      }
      return this.synth;
    }

    const AudioCtx = window.AudioContext || window.webkitAudioContext;
    this.audioCtx = new AudioCtx({ latencyHint: "interactive" });
    const sr = this.audioCtx.sampleRate;

    if (!this.wasmBytes) {
      const res = await fetch("./sound_hall_wasm.wasm");
      this.wasmBytes = await res.arrayBuffer();
    }

    // Re-instantiate at the exact AudioContext hardware sampleRate
    const prevScore = this.elScoreTextarea.value;
    if (this.synth) {
      this.synth.dispose();
    }
    this.synth = await SoundHallWasm.instantiate(this.wasmBytes, sr, 64);
    this.applySelectedPreset();
    this.synth.setMasterGain(Number(this.sliderVolume.value) / 100);
    this.synth.setReverbMix(Number(this.sliderReverb.value) / 100);
    this.synth.setChorusMix(Number(this.sliderChorus.value) / 100);
    if (prevScore.trim()) {
      this.synth.loadScore(prevScore);
    }

    this.analyser = this.audioCtx.createAnalyser();
    this.analyser.fftSize = 2048;
    this.analyser.smoothingTimeConstant = 0.78;
    this.timeDomainBuf = new Float32Array(this.analyser.fftSize);
    this.freqDomainBuf = new Uint8Array(this.analyser.frequencyBinCount);

    this.scriptNode = this.audioCtx.createScriptProcessor(512, 0, 2);
    this.scriptNode.onaudioprocess = (ev) => {
      const outL = ev.outputBuffer.getChannelData(0);
      const outR = ev.outputBuffer.getChannelData(1);
      if (this.synth) {
        this.synth.renderPlanar(outL, outR);
      } else {
        outL.fill(0);
        outR.fill(0);
      }
    };

    this.scriptNode.connect(this.analyser);
    this.analyser.connect(this.audioCtx.destination);
    await this.audioCtx.resume();

    this.elStatusEngine.textContent = `Running (${(sr / 1000).toFixed(1)} kHz)`;
    this.elStatusEngine.className = "pill-value status-running";
    return this.synth;
  }

  renderFamilyChips() {
    this.elFamilyChips.innerHTML = "";
    for (const fam of FAMILIES) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = `family-chip${fam === this.selectedFamily ? " active" : ""}`;
      btn.textContent =
        fam === "All"
          ? `All (${CATALOG.length})`
          : fam === WAGAKKI_FAMILY
            ? `${WAGAKKI_FAMILY} (${WAGAKKI_PRESETS.length})`
            : fam;
      btn.addEventListener("click", () => {
        this.selectedFamily = fam;
        for (const c of this.elFamilyChips.children) {
          c.classList.toggle("active", c === btn);
        }
        this.renderInstrumentList();
      });
      this.elFamilyChips.appendChild(btn);
    }
  }

  renderInstrumentList() {
    const q = this.searchQuery.trim().toLowerCase();
    const filtered = CATALOG.filter((p) => {
      if (this.selectedFamily === WAGAKKI_FAMILY) {
        if (p.bank !== WAGAKKI_BANK) return false;
      } else if (this.selectedFamily !== "All" && p.fam !== this.selectedFamily) {
        return false;
      }
      if (!q) return true;
      return (
        (p.bank === 0 && String(p.prog) === q) ||
        presetLabel(p).toLowerCase().includes(q) ||
        p.en.toLowerCase().includes(q) ||
        p.ja.toLowerCase().includes(q) ||
        p.fam.toLowerCase().includes(q) ||
        p.eng.toLowerCase().includes(q) ||
        (p.bank === WAGAKKI_BANK && (WAGAKKI_FAMILY.includes(q) || "wagakki".includes(q)))
      );
    });

    this.elInstrumentCount.textContent = `${filtered.length} 件`;
    this.elInstrumentList.innerHTML = "";

    for (const p of filtered) {
      const isSelected = p.bank === this.selectedBank && p.prog === this.selectedProgram;
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = `instrument-card${isSelected ? " selected" : ""}`;
      btn.setAttribute("role", "option");
      btn.setAttribute("aria-selected", isSelected ? "true" : "false");
      btn.dataset.program = String(p.prog);
      btn.dataset.bank = String(p.bank);

      const main = document.createElement("div");
      main.className = "inst-main";

      const titleRow = document.createElement("div");
      titleRow.className = "inst-title-row";

      const progSpan = document.createElement("span");
      progSpan.className = "inst-prog";
      progSpan.textContent = presetLabel(p);

      const jaSpan = document.createElement("span");
      jaSpan.className = "inst-name-ja";
      jaSpan.textContent = p.ja;

      titleRow.append(progSpan, jaSpan);

      const enSpan = document.createElement("span");
      enSpan.className = "inst-name-en";
      enSpan.textContent = p.en;

      main.append(titleRow, enSpan);

      const badges = document.createElement("div");
      badges.className = "inst-badges";

      const engSpan = document.createElement("span");
      engSpan.className = "badge-engine";
      engSpan.textContent = p.eng;

      const famSpan = document.createElement("span");
      famSpan.className = "badge-family";
      famSpan.textContent = p.bank === WAGAKKI_BANK ? `${WAGAKKI_FAMILY} · ${p.fam}` : p.fam;

      badges.append(engSpan, famSpan);
      btn.append(main, badges);

      btn.addEventListener("click", async () => {
        await this.selectInstrument(p.prog, this.chkAutoAudition.checked, p.bank);
      });

      this.elInstrumentList.appendChild(btn);
    }
  }

  /** Sends Bank Select + Program Change for the selected instrument on channel 1. */
  applySelectedPreset() {
    if (this.synth) {
      this.synth.bankProgramChange(0, this.selectedBank, this.selectedProgram);
    }
  }

  selectedPreset() {
    return findPreset(this.selectedBank, this.selectedProgram);
  }

  async selectInstrument(program, audition = false, bank = 0) {
    const preset = findPreset(bank, program & 0x7f);
    this.selectedBank = preset.bank;
    this.selectedProgram = preset.prog;

    this.elStatusPatch.textContent = `${presetLabel(preset)} ${preset.en} (${preset.ja})`;
    this.elReadoutEngineKind.textContent = `${preset.eng} Engine`;

    for (const card of this.elInstrumentList.children) {
      const isMatch =
        Number(card.dataset.program) === this.selectedProgram &&
        Number(card.dataset.bank) === this.selectedBank;
      card.classList.toggle("selected", isMatch);
      card.setAttribute("aria-selected", isMatch ? "true" : "false");
    }

    this.applySelectedPreset();

    if (audition) {
      await this.ensureAudioStarted();
      this.applySelectedPreset();
      this.triggerPreviewNote(preset.preview, 0.55);
    }
  }

  renderChannelMatrix() {
    this.elChannelMatrix.innerHTML = "";
    this.channelCells = [];
    for (let ch = 0; ch < 16; ch++) {
      const cell = document.createElement("div");
      cell.className = "channel-cell";

      const top = document.createElement("div");
      top.className = "ch-top";
      const chLabel = document.createElement("span");
      chLabel.textContent = ch === 9 ? "CH 10 (DRUM)" : `CH ${ch + 1}`;
      const noteSpan = document.createElement("span");
      noteSpan.className = "ch-note";
      noteSpan.textContent = "-";
      top.append(chLabel, noteSpan);

      const inst = document.createElement("div");
      inst.className = "ch-inst";
      inst.textContent = ch === 9 ? "Standard Drum Kit" : CATALOG[0].ja;

      const track = document.createElement("div");
      track.className = "ch-meter-track";
      const fill = document.createElement("div");
      fill.className = "ch-meter-fill";
      track.appendChild(fill);

      cell.append(top, inst, track);
      this.elChannelMatrix.appendChild(cell);
      this.channelCells.push({ cell, noteSpan, inst, fill });
    }
  }

  renderPianoKeyboard() {
    this.elPianoKeyboard.innerHTML = "";
    this.pianoKeyElements = new Map();

    const startNote = (this.baseOctave + 1) * 12; // e.g. baseOctave 3 -> C3 = 48
    const endNote = Math.min(127, startNote + 35); // 3 octaves (36 semitones)
    this.elLabelOctaveRange.textContent = `Range: ${midiToNoteName(startNote)} (${startNote}) – ${midiToNoteName(endNote)} (${endNote})`;

    const kbShortcuts = {
      0: "A",
      1: "W",
      2: "S",
      3: "E",
      4: "D",
      5: "F",
      6: "T",
      7: "G",
      8: "Y",
      9: "H",
      10: "U",
      11: "J",
      12: "K",
      13: "O",
      14: "L",
      15: "P",
      16: ";",
      17: "'",
    };

    const isBlack = (n) => [1, 3, 6, 8, 10].includes(n % 12);
    const whiteNotes = [];
    for (let n = startNote; n <= endNote; n++) {
      if (!isBlack(n)) whiteNotes.push(n);
    }

    const totalWhites = whiteNotes.length;
    let whiteIndex = 0;

    for (let n = startNote; n <= endNote; n++) {
      const black = isBlack(n);
      const key = document.createElement("button");
      key.type = "button";
      key.className = `piano-key ${black ? "black-key" : "white-key"}`;
      key.dataset.note = String(n);
      key.setAttribute("aria-label", `${midiToNoteName(n)} (MIDI ${n})`);

      if (black) {
        const leftPct = (whiteIndex / totalWhites) * 100;
        key.style.left = `${leftPct}%`;
      } else {
        whiteIndex++;
      }

      const noteLabel = document.createElement("span");
      noteLabel.className = "key-note-label";
      noteLabel.textContent = n % 12 === 0 || !black ? midiToNoteName(n) : "";

      const sc = kbShortcuts[n - startNote];
      if (sc) {
        const scLabel = document.createElement("span");
        scLabel.className = "key-shortcut-label";
        scLabel.textContent = sc;
        key.appendChild(scLabel);
      }
      key.appendChild(noteLabel);

      key.addEventListener("pointerdown", async (ev) => {
        ev.preventDefault();
        key.releasePointerCapture?.(ev.pointerId);
        this.pointerActiveNote = n;
        await this.pressNote(n);
      });

      key.addEventListener("pointerenter", async (ev) => {
        if (ev.buttons === 1 && this.pointerActiveNote !== null && this.pointerActiveNote !== n) {
          this.releaseNote(this.pointerActiveNote);
          this.pointerActiveNote = n;
          await this.pressNote(n);
        }
      });

      this.elPianoKeyboard.appendChild(key);
      this.pianoKeyElements.set(n, key);
    }
  }

  renderDrumPads() {
    this.elDrumPadsGrid.innerHTML = "";
    this.drumPadElements = new Map();

    for (const [note, en, ja, choke] of GM_DRUMS) {
      const pad = document.createElement("button");
      pad.type = "button";
      pad.className = "drum-pad";
      pad.dataset.note = String(note);

      const top = document.createElement("div");
      top.className = "drum-pad-top";
      const numSpan = document.createElement("span");
      numSpan.textContent = `MIDI ${note} (${midiToNoteName(note)})`;
      const chokeSpan = document.createElement("span");
      chokeSpan.textContent = choke;
      top.append(numSpan, chokeSpan);

      const jaSpan = document.createElement("div");
      jaSpan.className = "drum-pad-ja";
      jaSpan.textContent = ja;

      const enSpan = document.createElement("div");
      enSpan.className = "drum-pad-en";
      enSpan.textContent = en;

      pad.append(top, jaSpan, enSpan);

      pad.addEventListener("pointerdown", async (ev) => {
        ev.preventDefault();
        await this.ensureAudioStarted();
        this.synth.noteOn(9, note, this.keyVelocity);
        pad.classList.add("active");
        setTimeout(() => pad.classList.remove("active"), 140);
      });

      this.elDrumPadsGrid.appendChild(pad);
      this.drumPadElements.set(note, pad);
    }
  }

  async pressNote(note) {
    await this.ensureAudioStarted();
    this.userHeldNotes.add(note);
    this.applySelectedPreset();
    this.synth.noteOn(0, note, this.keyVelocity);
    this.pianoKeyElements.get(note)?.classList.add("active");
  }

  releaseNote(note) {
    this.userHeldNotes.delete(note);
    if (this.synth) {
      this.synth.noteOff(0, note);
    }
    this.pianoKeyElements.get(note)?.classList.remove("active");
  }

  async triggerPreviewNote(note = 60, durationSec = 0.6) {
    await this.ensureAudioStarted();
    this.applySelectedPreset();
    this.synth.noteOn(0, note, this.keyVelocity);
    setTimeout(() => {
      if (this.synth) this.synth.noteOff(0, note);
    }, durationSec * 1000);
  }

  async triggerPreviewChord() {
    await this.ensureAudioStarted();
    this.applySelectedPreset();
    const notes = [60, 64, 67, 71, 74]; // Cmaj9
    notes.forEach((n, idx) => {
      setTimeout(() => {
        if (this.synth) this.synth.noteOn(0, n, this.keyVelocity);
      }, idx * 55);
      setTimeout(() => {
        if (this.synth) this.synth.noteOff(0, n);
      }, idx * 55 + 950);
    });
  }

  bindEvents() {
    window.addEventListener("pointerup", () => {
      if (this.pointerActiveNote !== null) {
        this.releaseNote(this.pointerActiveNote);
        this.pointerActiveNote = null;
      }
    });

    this.elBtnPanic.addEventListener("click", () => {
      this.userHeldNotes.clear();
      this.kbHeldNotes.clear();
      if (this.synth) {
        this.synth.seqPause();
        this.synth.allNotesOff();
      }
      this.updatePlayButtonLabel(false);
    });

    this.elBtnAuditionNote.addEventListener("click", () =>
      this.triggerPreviewNote(this.selectedPreset()?.preview ?? 60, 0.75),
    );
    this.elBtnAuditionChord.addEventListener("click", () => this.triggerPreviewChord());

    this.elBtnSustain.addEventListener("click", async () => {
      this.sustainOn = !this.sustainOn;
      this.elBtnSustain.setAttribute("aria-pressed", this.sustainOn ? "true" : "false");
      this.elBtnSustain.textContent = `Sustain (CC64): ${this.sustainOn ? "ON" : "OFF"}`;
      await this.ensureAudioStarted();
      this.synth.controlChange(0, 64, this.sustainOn ? 127 : 0);
    });

    // Sliders
    this.sliderVolume.addEventListener("input", () => {
      const v = Number(this.sliderVolume.value);
      const txt = `${v}%`;
      this.valVolume.textContent = txt;
      syncSliderCss(this.sliderVolume, txt);
      if (this.synth) this.synth.setMasterGain(v / 100);
    });

    this.sliderReverb.addEventListener("input", () => {
      const v = Number(this.sliderReverb.value);
      const txt = `${v}%`;
      this.valReverb.textContent = txt;
      syncSliderCss(this.sliderReverb, txt);
      if (this.synth) this.synth.setReverbMix(v / 100);
    });

    this.sliderChorus.addEventListener("input", () => {
      const v = Number(this.sliderChorus.value);
      const txt = `${v}%`;
      this.valChorus.textContent = txt;
      syncSliderCss(this.sliderChorus, txt);
      if (this.synth) this.synth.setChorusMix(v / 100);
    });

    this.sliderVelocity.addEventListener("input", () => {
      const v = Number(this.sliderVelocity.value);
      this.keyVelocity = v;
      this.valVelocity.textContent = String(v);
      syncSliderCss(this.sliderVelocity, String(v));
    });

    this.sliderMod.addEventListener("input", () => {
      const v = Number(this.sliderMod.value);
      const pct = `${Math.round((v / 127) * 100)}%`;
      this.valMod.textContent = pct;
      syncSliderCss(this.sliderMod, pct);
      if (this.synth) this.synth.controlChange(0, 1, v);
    });

    this.sliderBend.addEventListener("input", () => {
      const cents = Number(this.sliderBend.value);
      const st = (cents / 100).toFixed(2);
      const txt = `${st} st`;
      this.valBend.textContent = txt;
      syncSliderCss(this.sliderBend, `${st} semitones`);
      if (this.synth) {
        const val14 = Math.round(8192 + (cents / 200) * 8191);
        this.synth.pitchBend(0, Math.max(0, Math.min(16383, val14)));
      }
    });

    this.sliderBend.addEventListener("dblclick", () => {
      this.sliderBend.value = "0";
      this.sliderBend.dispatchEvent(new Event("input"));
    });

    // Search
    this.inputSearch.addEventListener("input", () => {
      this.searchQuery = this.inputSearch.value;
      this.renderInstrumentList();
    });

    // Demo Buttons
    for (const btn of this.demoButtons) {
      btn.addEventListener("click", async () => {
        const idx = Number(btn.dataset.demo);
        for (const b of this.demoButtons) b.classList.toggle("active", b === btn);
        this.elScoreTextarea.value = DEMO_SCORES[idx];
        await this.ensureAudioStarted();
        const dur = this.synth.loadScore(DEMO_SCORES[idx]);
        if (dur >= 0) {
          this.setScoreStatus(true, `Loaded demo (${dur.toFixed(1)}s)`);
          this.synth.seqPlay();
          this.updatePlayButtonLabel(true);
        }
      });
    }

    // Sequencer Transport
    this.elBtnSeqPlay.addEventListener("click", async () => {
      await this.ensureAudioStarted();
      if (this.synth.seqIsPlaying()) {
        this.synth.seqPause();
        this.updatePlayButtonLabel(false);
      } else {
        if (this.synth.seqDurationSec() <= 0.01) {
          this.applyScoreFromEditor(false);
        }
        this.synth.seqPlay();
        this.updatePlayButtonLabel(true);
      }
    });

    this.elBtnSeqStop.addEventListener("click", () => {
      if (this.synth) {
        this.synth.seqStop();
      }
      this.updatePlayButtonLabel(false);
      this.updateTimelineReadout();
    });

    this.sliderTimeline.addEventListener("pointerdown", () => {
      this.isTimelineScrubbing = true;
    });
    this.sliderTimeline.addEventListener("pointerup", () => {
      this.isTimelineScrubbing = false;
    });
    this.sliderTimeline.addEventListener("input", () => {
      if (!this.synth) return;
      const dur = this.synth.seqDurationSec();
      const target = (Number(this.sliderTimeline.value) / 1000) * dur;
      this.synth.seqSeek(target);
      syncSliderCss(this.sliderTimeline, `${target.toFixed(1)} seconds`);
      this.valTimeline.textContent = `${formatTime(target)} / ${formatTime(dur)}`;
    });

    // Apply Score from Editor
    this.elBtnApplyScore.addEventListener("click", async () => {
      await this.ensureAudioStarted();
      this.applyScoreFromEditor(true);
    });

    // File Import (.mid / .shs)
    this.inputFileImport.addEventListener("change", async (ev) => {
      const file = ev.target.files?.[0];
      if (!file) return;
      await this.importFile(file);
      this.inputFileImport.value = "";
    });

    // Drag-and-drop .mid / .shs onto window
    window.addEventListener("dragover", (ev) => ev.preventDefault());
    window.addEventListener("drop", async (ev) => {
      ev.preventDefault();
      const file = ev.dataTransfer?.files?.[0];
      if (file) await this.importFile(file);
    });

    // WAV Export
    this.elBtnExportWav.addEventListener("click", async () => {
      if (!this.synth) await this.preloadWasm();
      if (!this.synth) return;
      if (this.synth.seqDurationSec() <= 0.01) {
        this.synth.loadScore(this.elScoreTextarea.value);
      }
      const prevText = this.elBtnExportWav.textContent;
      this.elBtnExportWav.textContent = "⏳ Rendering WAV...";
      setTimeout(() => {
        const wavBytes = this.synth.exportSongWav(true);
        this.elBtnExportWav.textContent = prevText;
        if (!wavBytes) {
          this.setScoreStatus(false, "WAV export failed");
          return;
        }
        const blob = new Blob([wavBytes], { type: "audio/wav" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = "sound-hall-export.wav";
        document.body.appendChild(a);
        a.click();
        a.remove();
        URL.revokeObjectURL(url);
        this.setScoreStatus(true, `Exported WAV (${(wavBytes.length / 1024).toFixed(0)} KB)`);
      }, 20);
    });

    // Tabs: Piano vs Drums
    this.elTabPiano.addEventListener("click", () => {
      this.elTabPiano.classList.add("active");
      this.elTabDrums.classList.remove("active");
      this.elViewPiano.hidden = false;
      this.elPianoToolbar.hidden = false;
      this.elViewDrums.hidden = true;
    });

    this.elTabDrums.addEventListener("click", () => {
      this.elTabDrums.classList.add("active");
      this.elTabPiano.classList.remove("active");
      this.elViewDrums.hidden = false;
      this.elViewPiano.hidden = true;
      this.elPianoToolbar.hidden = true;
    });

    // Octave shift
    this.elBtnOctDown.addEventListener("click", () => {
      this.baseOctave = Math.max(0, this.baseOctave - 1);
      this.renderPianoKeyboard();
    });
    this.elBtnOctUp.addEventListener("click", () => {
      this.baseOctave = Math.min(6, this.baseOctave + 1);
      this.renderPianoKeyboard();
    });

    // Computer Keyboard Playing
    const keyMap = {
      KeyA: 0,
      KeyW: 1,
      KeyS: 2,
      KeyE: 3,
      KeyD: 4,
      KeyF: 5,
      KeyT: 6,
      KeyG: 7,
      KeyY: 8,
      KeyH: 9,
      KeyU: 10,
      KeyJ: 11,
      KeyK: 12,
      KeyO: 13,
      KeyL: 14,
      KeyP: 15,
      Semicolon: 16,
      Quote: 17,
    };

    window.addEventListener("keydown", async (ev) => {
      const tag = document.activeElement?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || ev.metaKey || ev.ctrlKey || ev.altKey) {
        return;
      }
      if (ev.repeat) return;

      if (ev.code === "KeyZ") {
        this.baseOctave = Math.max(0, this.baseOctave - 1);
        this.renderPianoKeyboard();
        return;
      }
      if (ev.code === "KeyX") {
        this.baseOctave = Math.min(6, this.baseOctave + 1);
        this.renderPianoKeyboard();
        return;
      }

      if (ev.code in keyMap) {
        ev.preventDefault();
        const note = Math.min(127, (this.baseOctave + 1) * 12 + keyMap[ev.code]);
        this.kbHeldNotes.set(ev.code, note);
        await this.pressNote(note);
      }
    });

    window.addEventListener("keyup", (ev) => {
      if (this.kbHeldNotes.has(ev.code)) {
        const note = this.kbHeldNotes.get(ev.code);
        this.kbHeldNotes.delete(ev.code);
        this.releaseNote(note);
      }
    });
  }

  applyScoreFromEditor(autoPlay = true) {
    if (!this.synth) return;
    const text = this.elScoreTextarea.value;
    const dur = this.synth.loadScore(text);
    if (dur < 0) {
      this.setScoreStatus(false, this.synth.lastError() || "Score parse error");
      return;
    }
    this.setScoreStatus(true, `Compiled OK (${dur.toFixed(1)}s)`);
    if (autoPlay) {
      this.synth.seqPlay();
      this.updatePlayButtonLabel(true);
    }
    this.updateTimelineReadout();
  }

  async importFile(file) {
    await this.ensureAudioStarted();
    const name = file.name.toLowerCase();
    if (name.endsWith(".mid") || name.endsWith(".midi")) {
      const buf = new Uint8Array(await file.arrayBuffer());
      const dur = this.synth.loadSmf(buf);
      if (dur < 0) {
        this.setScoreStatus(false, `MIDI Error: ${this.synth.lastError()}`);
        return;
      }
      this.setScoreStatus(true, `Loaded MIDI: ${file.name} (${dur.toFixed(1)}s)`);
      this.synth.seqPlay();
      this.updatePlayButtonLabel(true);
    } else {
      const text = await file.text();
      this.elScoreTextarea.value = text;
      this.applyScoreFromEditor(true);
    }
  }

  setScoreStatus(ok, message) {
    this.elScoreStatus.textContent = message;
    this.elScoreStatus.className = ok ? "score-status-ok" : "score-status-err";
  }

  updatePlayButtonLabel(playing) {
    this.elBtnSeqPlay.textContent = playing ? "⏸ 一時停止 (Pause)" : "▶ 再生 (Play)";
  }

  updateTimelineReadout() {
    if (!this.synth) return;
    const pos = this.synth.seqPositionSec();
    const dur = this.synth.seqDurationSec();
    this.valTimeline.textContent = `${formatTime(pos)} / ${formatTime(dur)}`;
    if (!this.isTimelineScrubbing) {
      const permille = dur > 0.001 ? Math.min(1000, Math.round((pos / dur) * 1000)) : 0;
      this.sliderTimeline.value = String(permille);
      syncSliderCss(this.sliderTimeline, `${pos.toFixed(1)} seconds`);
    }
  }

  async initWebMidi() {
    if (!navigator.requestMIDIAccess) {
      this.elStatusMidi.textContent = "Unsupported";
      return;
    }
    try {
      const midi = await navigator.requestMIDIAccess();
      const bindInputs = () => {
        let count = 0;
        for (const input of midi.inputs.values()) {
          count++;
          input.onmidimessage = async (msg) => {
            await this.ensureAudioStarted();
            const [status, d1 = 0, d2 = 0] = msg.data;
            const cmd = status & 0xf0;
            const ch = status & 0x0f;
            if (cmd === 0x90 && d2 > 0) {
              this.synth.noteOn(ch, d1, d2);
            } else if (cmd === 0x80 || (cmd === 0x90 && d2 === 0)) {
              this.synth.noteOff(ch, d1);
            } else if (cmd === 0xb0) {
              this.synth.controlChange(ch, d1, d2);
            } else if (cmd === 0xc0) {
              this.synth.programChange(ch, d1);
            } else if (cmd === 0xe0) {
              this.synth.pitchBend(ch, (d2 << 7) | d1);
            }
          };
        }
        this.elStatusMidi.textContent = count > 0 ? `${count} Device(s)` : "Ready (0 connected)";
      };
      bindInputs();
      midi.onstatechange = bindInputs;
    } catch {
      this.elStatusMidi.textContent = "Blocked/Unavailable";
    }
  }

  animationLoop() {
    if (this.synth) {
      const voices = this.synth.activeVoices();
      this.elStatusVoices.textContent = `${voices} / 64`;

      const playing = this.synth.seqIsPlaying();
      this.updatePlayButtonLabel(playing);
      this.updateTimelineReadout();

      // Update 16-Channel Live Matrix
      for (let ch = 0; ch < 16; ch++) {
        const entry = this.channelCells[ch];
        const act = this.synth.channelActivity(ch);
        const prog = this.synth.channelProgram(ch);
        const bank = this.synth.channelBank(ch);
        const lastNote = this.synth.channelLastNote(ch);

        entry.fill.style.inlineSize = `${Math.round(act * 100)}%`;
        entry.cell.classList.toggle("active-ch", act > 0.05);
        entry.noteSpan.textContent = act > 0.02 ? midiToNoteName(lastNote) : "-";
        if (ch !== 9) {
          const p = findPreset(bank, prog);
          entry.inst.textContent = p ? `${presetLabel(p)} ${p.ja}` : `#${prog}`;
        }
      }

      // Highlight active notes on the Piano Keyboard
      const lowMask = this.synth.activeNotesLow();
      const highMask = this.synth.activeNotesHigh();
      for (const [note, keyEl] of this.pianoKeyElements.entries()) {
        const bitOn =
          note < 64
            ? (lowMask & (1n << BigInt(note))) !== 0n
            : (highMask & (1n << BigInt(note - 64))) !== 0n;
        keyEl.classList.toggle("active", bitOn || this.userHeldNotes.has(note));
      }
    }

    this.drawScope();
    this.drawSpectrum();
    requestAnimationFrame(() => this.animationLoop());
  }

  drawScope() {
    const canvas = this.canvasScope;
    const ctx = this.ctxScope;
    const w = canvas.width;
    const h = canvas.height;

    ctx.fillStyle = "#070b12";
    ctx.fillRect(0, 0, w, h);

    // Subtle grid lines
    ctx.strokeStyle = "#162032";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, h / 2);
    ctx.lineTo(w, h / 2);
    ctx.moveTo(0, h / 4);
    ctx.lineTo(w, h / 4);
    ctx.moveTo(0, (h * 3) / 4);
    ctx.lineTo(w, (h * 3) / 4);
    ctx.stroke();

    if (!this.analyser || !this.timeDomainBuf) {
      return;
    }

    this.analyser.getFloatTimeDomainData(this.timeDomainBuf);
    const buf = this.timeDomainBuf;

    // Peak readout
    let peak = 0;
    for (let i = 0; i < buf.length; i++) {
      const a = Math.abs(buf[i]);
      if (a > peak) peak = a;
    }
    const db = peak > 1e-5 ? (20 * Math.log10(peak)).toFixed(1) : "-∞";
    this.elReadoutPeak.textContent = `Peak: ${db} dB`;

    // Rising-edge zero-crossing trigger for stable waveform display
    let triggerIdx = 0;
    for (let i = 1; i < buf.length / 2; i++) {
      if (buf[i - 1] < 0 && buf[i] >= 0) {
        triggerIdx = i;
        break;
      }
    }

    const drawSamples = Math.min(768, buf.length - triggerIdx);
    ctx.strokeStyle = "#38bdf8";
    ctx.lineWidth = 2;
    ctx.beginPath();
    for (let i = 0; i < drawSamples; i++) {
      const x = (i / (drawSamples - 1)) * w;
      const y = h / 2 - buf[triggerIdx + i] * (h * 0.44);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();
  }

  drawSpectrum() {
    const canvas = this.canvasSpectrum;
    const ctx = this.ctxSpectrum;
    const w = canvas.width;
    const h = canvas.height;

    ctx.fillStyle = "#070b12";
    ctx.fillRect(0, 0, w, h);

    ctx.strokeStyle = "#162032";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, h * 0.25);
    ctx.lineTo(w, h * 0.25);
    ctx.moveTo(0, h * 0.5);
    ctx.lineTo(w, h * 0.5);
    ctx.moveTo(0, h * 0.75);
    ctx.lineTo(w, h * 0.75);
    ctx.stroke();

    if (!this.analyser || !this.freqDomainBuf) {
      return;
    }

    this.analyser.getByteFrequencyData(this.freqDomainBuf);
    const bins = this.freqDomainBuf;
    const barCount = 64;
    const barSlot = w / barCount;
    const barW = Math.max(2, barSlot - 2);

    for (let b = 0; b < barCount; b++) {
      // Logarithmic frequency bin mapping
      const norm = b / barCount;
      const binIdx = Math.min(
        bins.length - 1,
        Math.floor(Math.pow(norm, 1.85) * (bins.length * 0.55)) + b
      );
      const val = bins[binIdx] / 255;

      if (val > this.spectrumPeaks[b]) {
        this.spectrumPeaks[b] = val;
      } else {
        this.spectrumPeaks[b] = Math.max(0, this.spectrumPeaks[b] - 0.008);
      }

      const barH = val * (h - 14);
      const x = b * barSlot + 1;
      const y = h - barH;

      const grad = ctx.createLinearGradient(0, h, 0, 0);
      grad.addColorStop(0, "#0284c7");
      grad.addColorStop(0.65, "#38bdf8");
      grad.addColorStop(1, "#34d399");

      ctx.fillStyle = grad;
      ctx.fillRect(x, y, barW, barH);

      // Peak hold cap
      if (this.spectrumPeaks[b] > 0.02) {
        const py = h - this.spectrumPeaks[b] * (h - 14) - 2;
        ctx.fillStyle = "#f8fafc";
        ctx.fillRect(x, Math.max(2, py), barW, 2);
      }
    }
  }
}

window.addEventListener("DOMContentLoaded", () => {
  new SoundHallStudioApp();
});
