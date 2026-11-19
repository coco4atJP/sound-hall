# SoundHall (`sound-hall`)

[![CI](https://github.com/coco4atJP/sound-hall/actions/workflows/ci.yml/badge.svg)](https://github.com/coco4atJP/sound-hall/actions/workflows/ci.yml)

外部クレート依存ゼロ・外部音声サンプル（PCM/SF2）完全不要で、**あらゆる楽器の音をリアルタイム再現する超軽量・物理モデリング＋ハイブリッド音源エンジン**（Rust + WebAssembly 対応）。

General MIDI（GM1）準拠の**全128音色**（ピアノ、鍵盤打楽器、オルガン、ギター、ベース、弦楽器、ブラス、木管、民族楽器、シンセ、効果音）および**47種類の標準ドラムキット**を、純粋な DSP アルゴリズムのみで合成します。

---

## 主な特徴

- **ゼロ依存（Zero External Dependencies）**: Rust 標準ライブラリ（`std`）のみで構成され、`aarch64`/`x86_64` ネイティブと `wasm32-unknown-unknown` の両方でビルド可能。
- **ゼロサンプル（Zero PCM Samples）**: Wave/SoundFont ファイル不要。コードと計算のみで音を鳴らすためバイナリもメモリも超軽量。
- **8つの統一合成エンジン ＋ 専用物理ドラムエンジン**:
  1. `PluckedString` — Extended Karplus-Strong デジタル導波管（4点エルミート補間・ピック位置コム・剛性分散オールパス・シタール/三味線用バズ非線形・ボディ共鳴器）
  2. `HammeredString` — ユニゾン複数弦導波管＋フェルトハンマー特性＋キータッチ打音＋2段階減衰（グランドピアノ・ホンキートンク等）
  3. `BowedString` — Slip-Stick 摩擦導波管＋ロジンノイズ＋木製胴体フォルマント＋アンサンブル・デチューン（ヴァイオリン〜コントラバス）
  4. `WindBore` — 管楽器導波管（エッジトーン気流 `FluteJet`・円筒奇数倍音/円錐 `SingleReed`・鼻腔フォルマント `DoubleReed`・金管リップリード `LipReed`）
  5. `ModalPercussion` — 最大10モード並列2次共振器バンク＋ベロシティ連動マレットインパルス（マリンバ・ヴィブラフォン・ティンパニ・和太鼓等）
  6. `Fm4Op` — 4オペレータ位相変調シンセ（`Stack4` / `Dual2x2` / `ThreeToOne` / `Parallel`、フィードバック、固定周波数対応）
  7. `AdditiveOrgan` — ハモンド9ドローバー加算合成＋キークリック＋倍音パーカッション＋ロータリースピーカー＋オーバードライブ
  8. `FormantSubtractive` — PolyBLEP アンチエイリアス発振器（最大5ボイス SuperSaw）＋ Cytomic ZDF SVF フィルタ＋母音フォルマントバンク＋ピッチエンベロープ
  9. `DrumVoice` — 47種類 GM パーカッション専用物理合成（膜ピッチスイープ・スネアワイヤーノイズ・6発振器金属シンバル群・チョークグループ対応）
- **空間エフェクト**: ステレオ・クアドラチャ・コーラス ＋ Freeverb/Schroeder 方式ステレオ・ホールリバーブ ＋ マスターソフトリミッター。
- **MIDI / 独自テキストスコア対応**:
  - Standard MIDI File（`.mid`, Format 0 / 1）パーサー＆サンプル精度シーケンサー
  - 人間が読み書きできるマルチトラックテキストスコア（`.shs`）と5曲の内蔵デモ

---

## ワークスペース構成

```text
sound-hall/
├── Cargo.toml
├── README.md
├── crates/
│   ├── sound-hall-core/   # コアDSP・8合成エンジン・GM 128音色+47ドラム・SMF/SHS・WAV書き出し
│   ├── sound-hall-wasm/   # wasm32-unknown-unknown C-ABI バインディング & Web Audio AudioWorklet ラッパー
│   └── sound-hall-cli/    # CLIツール（楽器一覧・試聴・MIDI/SHS→WAV変換・ベンチマーク）
└── examples/
    ├── orchestral_showcase.shs
    └── japanese_ensemble.shs
```

---

## CLI の使い方

### クローンとビルド

```bash
git clone https://github.com/coco4atJP/sound-hall.git
cd sound-hall
cargo build --release --workspace
```

### 楽器・ドラム一覧と詳細確認

```bash
# GM全128音色の一覧を表示
./target/release/sound-hall list

# ファミリーやキーワード（日本語/英語/プログラム番号）で絞り込み
./target/release/sound-hall list --family Ethnic
./target/release/sound-hall list --search 尺八

# GM標準ドラムキット47音色を表示
./target/release/sound-hall list --drums

# 楽器の詳細パラメータを確認
./target/release/sound-hall info 0
./target/release/sound-hall info shakuhachi
```

### 単音・和音・内蔵デモの試聴 / WAV 書き出し

```bash
# 単音プレビュー（--play で即座に再生、--out でWAV出力）
./target/release/sound-hall note "Acoustic Grand Piano" C4 --duration 1.5 --out piano.wav
./target/release/sound-hall note 77 D4 --duration 2.0 --play

# 和音（アルペジオ付き）
./target/release/sound-hall chord 46 C4,E4,G4,B4 --arpeggio 0.06 --out harp.wav

# 内蔵デモ一覧とレンダリング / 再生
./target/release/sound-hall demo list
./target/release/sound-hall demo orchestra -o orchestra.wav
./target/release/sound-hall demo wagakki --play
```

### `.mid` / `.shs` ファイルのレンダリング・再生とベンチマーク

```bash
# スコア (.shs) または MIDI (.mid) を WAV にレンダリング
./target/release/sound-hall render examples/japanese_ensemble.shs -o japanese.wav --normalize
./target/release/sound-hall render song.mid -o song.wav --sample-rate 48000 --reverb 0.35

# 即時再生（macOS では afplay を使用）
./target/release/sound-hall play examples/orchestral_showcase.shs

# DSP パフォーマンスベンチマーク（32ボイス持続同時発音・検証済みアクティブボイス数表示）
./target/release/sound-hall benchmark --voices 32 --seconds 10
```

---

## `.shs` (SoundHall Score) 記法

```text
tempo 94

track Koto ch=0 program=107 vol=102 pan=44 rev=42
o4 l8 q8 v92
d8 e8 f8 a8 >d4< a8 f8 | [d a >d<]1

track Drums ch=9 vol=96 pan=64 rev=24
l8 v90
([36,42]8 42:8 [38,42]8 42:8)*2
```

- `tempo <BPM>`: テンポ設定
- `track <name> ch=<0-15> program=<0-127|名前> vol=<0-127> pan=<0-127> rev=<0-127> cho=<0-127>`
- 音符: `c..b`（`#`/`+` 嬰記号、`b`/`-` 変記号）、休符 `r`、付点 `.`、長さ `l1..l64`
- オクターブ: `o0..o8`, `>` (+1), `<` (-1)
- ベロシティ `v0..v127`、ゲート比率 `q1..q8`
- 和音: `[c e g]4`、ドラムMIDI番号: `36:8` や `[36,42]8`
- 反復: `( ... )*4`、`#` 以降はコメント

---

## Rust ライブラリとしての利用

```rust
use sound_hall_core::{SoundHallEngine, write_wav_i16};
use std::fs::File;

fn main() -> std::io::Result<()> {
    let mut engine = SoundHallEngine::new(44100.0);
    engine.program_change(0, 0);   // Acoustic Grand Piano
    engine.note_on(0, 60, 100);    // C4
    engine.note_on(0, 64, 96);     // E4
    engine.note_on(0, 67, 96);     // G4

    let mut stereo = vec![0.0_f32; 44100 * 2];
    engine.render_stereo(&mut stereo[..22050 * 2]);
    engine.all_notes_off();
    engine.render_stereo(&mut stereo[22050 * 2..]);

    let mut file = File::create("out.wav")?;
    write_wav_i16(&mut file, 44100, 2, &stereo, true)
}
```

---

## WebAssembly / Web Audio API (`sound-hall-wasm`)

外部の `wasm-bindgen` 等を使わず、純粋な C-ABI（`sh_engine_new`, `sh_note_on`, `sh_render_planar` 等）をエクスポートしています。

```bash
rustup target add wasm32-unknown-unknown
cargo build -p sound-hall-wasm --target wasm32-unknown-unknown --profile wasm-release
```

生成された `target/wasm32-unknown-unknown/wasm-release/sound_hall_wasm.wasm` を、同梱の ESM ラッパー [`crates/sound-hall-wasm/web/sound-hall.js`](crates/sound-hall-wasm/web/sound-hall.js) および [`crates/sound-hall-wasm/web/worklet-processor.js`](crates/sound-hall-wasm/web/worklet-processor.js) からそのまま `AudioWorklet` で鳴らすことができます。

---

## テスト

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

---

## ライセンス

Dual-licensed under [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE).

