# BurstJudge (05 — AtışTriyajı)

Burst ve çoklu çekim karelerini **TIFF/EXIF IFD ayrıştırması**, **gömülü JPEG
önizleme çıkarımı** ve **DCT tabanlı algısal hash** ile gruplayan; her gruptan
en güçlü kareyi *öneren* terminal triyaj aracı.

**Hiçbir dosyayı silmez, taşımaz veya tam çözmez.** Görüntü pikselleri hiçbir
zaman saklanmaz; rapor yalnız hash, puan, boyut ve yol tutar.

---

## Özellikler

MVP kapsamındaki her özellik madde madde:

- **TIFF 6.0 + Exif 2.3 IFD ayrıştırıcı** — az öncelikli (`II`) ve büyük
  öncelikli (`MM`) byte sırası, 13 alan tipi, IFD0 + `ExifIFDPointer` (0x8769)
  + `GPSInfoIFDPointer` (0x8825) dizinleri. Okunan alanlar: `Make`, `Model`,
  `DateTimeOriginal`, `ExposureTime`, `FNumber`, `ISO`, `FocalLength`,
  `Orientation`, `ImageWidth`/`ImageLength`, `PixelXDimension`/`PixelYDimension`,
  `Compression`, `ExposureBiasValue`, `MeteringMode`, `ExposureProgram`,
  `Flash`, `WhiteBalance`, `StripByteCounts`, `Software`.
- **BigTIFF reddi** — sürüm 43 imzası (`II+` / `MM+`) açık hata ile reddedilir;
  sessizce yanlış offset okunmaz.
- **Gömülü JPEG önizleme çıkarımı** — önce `JPEGInterchangeFormat` (0x0201) +
  `JPEGInterchangeFormatLength` (0x0202), yoksa `Compression` = 7 iken
  `StripOffsets` (0x0111) + `StripByteCounts` (0x0117). Bulunan aralık
  **çözülmeden** `Stream` üzerinden parça parça kopyalanır (64 KiB tampon).
- **Baseline JPEG kod çözücü (kendi yazıldı)** — `SOF0`/`SOF1`; `DQT`, `DHT`,
  `DRI`/`RSTn`, `SOS`, byte-stuffing, kanonik Huffman, ters DCT, 4:4:4 /
  4:2:2 / 4:2:0 alt örnekleme. Yalnız **luma (Y)** kanalı çözülür.
- **Algısal hash** — 8×8 kutu ortalaması → 8×8 DCT-II → DC hariç 63 katsayının
  ortalamasına göre eşik → 64 bit. Kayan nokta hassasiyeti belgelenmiştir
  (bkz. `src/bhash.rs`).
- **Benzerlik gruplama** — union-find (yol sıkıştırma + ağırlık birleştirme),
  hamming mesafesi eşiği, 16-bit önek tabanlı hızlı aday üretimi ve
  **çakışma tespiti** (zincir birleşme görünür kılınır).
- **Metadata tabanlı puanlama** — `keskinlik`, `pozlama`, `kirpma`,
  `kompozisyon` (yöne göre kadraj), `iso_dengesi`. Her bileşenin gerekçesi
  raporda metin olarak yazılır; eksik alan bileşeni hesaba katılmaz.
- **Tam çözümlemeden tarama** — dosyanın yalnız dizin bölümü okunur; ham sensör
  verisi hiç okunmaz. `okunan bayt / dosya boyutu` oranı her çalıştırmada
  raporlanır (demo klasöründe **%0,08**).
- **Komut satırı** — `clap` ile `group`, `judge`, `export`, `info` alt komutları.
- **`serde` + `serde_json` rapor** — sema sürümlü, belgelenmiş JSON çıktısı.
- **Kaynağa dokunmayan dışa aktarma** — hedef kaynakla çakışıyorsa reddedilir,
  kopyalama geçici dosya üzerinden `rename` ile tamamlanır, ad çakışmasında
  numaralandırılır.

---

## Kurulum

Gereksinim: Rust **1.74+** (MSRV). Geliştirme ortamında `cargo 1.98.1` ile
derlendi ve test edildi.

```bash
cargo build --release
```

Gerçek çıktı:

```text
   Compiling burstjudge v0.1.0 (%USERPROFILE%\Desktop\Projeler\projects\05-burstjudge)
    Finished `release` profile [optimized] target(s) in 19.23s
```

Üretilen tek dosyalık ikili: `target/release/burstjudge.exe`
(**1 234 111 bayt**, yaklaşık 1,18 MiB).

Programı sistem geneline kurmak için:

```bash
cargo install --path .
```

Windows'ta `winget install Rustlang.Rustup` ile kurulan araç zinciri
`cargo` + `rustc` + `dlltool` gerektirir. **PowerShell kullanıcıları için**:

```powershell
$env:PATH = "%USERPROFILE%\.cargo\bin;%USERPROFILE%\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin;" + $env:PATH
```

Bu satır olmadan `cargo build --release` şu hatayla durur:

```text
error: error calling dlltool 'dlltool.exe': program not found
```

---

## Kullanım

Aşağıdaki bütün komutlar **gerçekten çalıştırıldı**; çıktılar birebir
kopyalandı. Demo klasörü `examples/ornek_klasor.rs` ile üretildi: üç kompozisyon
grubundan dokuz TIFF/EXIF karesi (her biri 2 MiB) ve bir bozuk dosya.

```bash
cargo run --release --example ornek_klasor -- %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme 9
```

```text
9 adet örnek kare yazıldı: %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme
```

### 1. `group` — tara ve grupla

```bash
burstjudge group %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme
```

```text
BurstJudge 0.1.0 — grup taraması
  kök           : %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme
  eşik          : 8
  dosya         : 10
  okunan bayt   : 15411 / 18874368 (0.0817%)
  önizleme      : 9 bulundu, 0 yok
  hash          : 9 üretildi
  atlanan       : 1
  grup          : 3
  çakışmalı grup: 0
```

`okunan bayt` satırı **kısmi okumanın kanıtıdır**: 18 MiB dosyaların toplamı
%0,08'i okunmuştur. Bozuk dosya taramayı durdurmamış, `atlanan: 1` olarak
sayılmıştır.

### 2. `info` — tek dosyanın özeti

```bash
burstjudge info %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme\IMG_portre_0001.CR2
```

```text
dosya        : %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme\IMG_portre_0001.CR2
boyut        : 2097152 bayt
okunan       : 834 bayt (0.0398%)
kamera       : Canon Test Kamera
çekim        : 2026-09-29T10:30:12
pozlama      : 1/400 f/2.8 ISO 200
odak         : 40 mm
boyut/yön    : 6000x4000 yön Some(1)
en-boy       : Some(1.5)
önizleme     : var (Some(580) bayt)
hash         : 7ffaffeaffaaffaa
puan         : 58.8
  - keskinlik      10.7  (ağırlık 0.30) 1/400 s, 40 mm -> hareket riski
  - pozlama        72.6  (ağırlık 0.25) ISO 200 -> ışık
  - kirpma         24.6  (ağırlık 0.10) 40 mm -> kadraja alınmışlık
  - kompozisyon   100.0  (ağırlık 0.20) en-boy 1.50 -> kadraj
  - iso_dengesi   100.0  (ağırlık 0.15) ISO 200 -> gürültü dengesi
```

Gömülü önizlemeyi tek başına `.jpg` olarak da yazabilir:

```bash
burstjudge info %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme\IMG_portre_0001.CR2 --onizleme-yaz %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme\onizleme.jpg
```

```text
  580 bayt yazıldı
önizleme yazıldı: %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme\onizleme.jpg
```

### 3. `judge` — puanla ve öner

```bash
burstjudge judge %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme --cikti %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme\rapor.json
```

```text
rapor yazıldı: %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme\rapor.json
```

Raporun başı (gerçek `rapor.json`):

```json
{
  "surum": "0.1.0",
  "sema": 1,
  "kok": "C:\\Users\\xXx\\AppData\\Local\\Temp\\opencode\\bj-readme",
  "esik": 8,
  "istatistik": {
    "bulunan_dosya": 10,
    "meta_okunan": 9,
    "onizleme_bulunan": 9,
    "onizleme_yok": 0,
    "onizleme_bozuk": 0,
    "hash_uretilen": 9,
    "atlanan": 1,
    "toplam_okunan_bayt": 15411,
    "toplam_dosya_boyutu": 18874368
  },
  "gruplar": [
    {
```

Bir grubun içi (üç üyenin puanı ve öneri gerekçesi):

```json
      "kimlik": 0,
      "oneri": "C:\\Users\\xXx\\AppData\\Local\\Temp\\opencode\\bj-readme\\IMG_manzara_0002.CR2",
      "gerekce": "IMG_manzara_0002.CR2: toplam 59.5 puan (keskinlik 20, pozlama 63, kirpma 30, kompozisyon 100, iso_dengesi 97)",
      "cakismali": false,
      "en_uzak_mesafe": 8,
```

### 4. `export` — önerileri kopyala (kaynak değişmez)

```bash
burstjudge export %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme\rapor.json %USERPROFILE%\AppData\Local\Temp\opencode\bj-secili
```

```text
grup   0 -> %USERPROFILE%\AppData\Local\Temp\opencode\bj-secili\IMG_manzara_0002.CR2 (2097152 bayt, puan 59.5)
grup   1 -> %USERPROFILE%\AppData\Local\Temp\opencode\bj-secili\IMG_mimari_0002.CR2 (2097152 bayt, puan 59.5)
grup   2 -> %USERPROFILE%\AppData\Local\Temp\opencode\bj-secili\IMG_portre_0002.CR2 (2097152 bayt, puan 59.5)
toplam 3 kare dışa aktarıldı
```

Yalnız bir grubu dışa aktarmak için:

```bash
burstjudge export %USERPROFILE%\AppData\Local\Temp\opencode\bj-readme\rapor.json %USERPROFILE%\AppData\Local\Temp\opencode\bj-secili2 --grup 0
```

```text
grup   0 -> %USERPROFILE%\AppData\Local\Temp\opencode\bj-secili2\IMG_manzara_0002.CR2 (2097152 bayt, puan 59.5)
toplam 1 kare dışa aktarıldı
```

### 5. `--help`

```bash
burstjudge --help
```

```text
Burst ve coklu cekim karelerini TIFF/EXIF IFD ayristirmasi, gomulu JPEG onizleme ve DCT tabanli algisal hash ile gruplayan, hicbir dosyayi tasimayan terminal triyaj araci.

Usage: burstjudge.exe <COMMAND>

Commands:
  group   Klasörü tara, benzer kareleri grupla ve özet yaz
  judge   Klasörü tara, puanla ve her grupta en iyi kareyi öner
  export  Rapor önerilerini yeni klasöre kopyala (kaynak değişmez)
  info    Tek bir dosyanın IFD/EXIF özetini yaz
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

---

## Test

```bash
cargo test
```

Gerçek sonuç:

```text
running 217 tests
...
test result: ok. 217 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s

     Running tests\cli_entegrasyon.rs
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.63s

     Running tests\tarama_entegrasyon.rs
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.74s

   Doc-tests burstjudge
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**Toplam: 252 test, 0 başarısız** (217 birim + 16 CLI + 19 entegrasyon).

Diğer kalite kapıları:

```bash
cargo clippy --all-targets -- -D warnings   # uyarısız
cargo fmt --check                          # biçim farkı yok
```

### Test vektörleri

Ortamda gerçek `.CR2`/`.NEF` dosyası olmadığı ve `image`/`tiff`/`jpeg-decoder`
crate'leri bağımlılık politikası nedeniyle yasak olduğu için testler **kendi
üreticimizi** kullanır (`src/jpeg_test_veri.rs`, `src/ornek_veri.rs`):

- **Geçerli baseline JPEG** — ileri DCT + standart Annex K Huffman tabloları ile
  kodlanır, sonra **kendi çözücümüzle geri çözülür** (round-trip). Düz, dikey
  şeritli, yatay şeritli, damalı ve radyal desenler için piksel doğruluğu ve
  hash üretimi sınanır.
- **Geçerli TIFF/EXIF** — iki geçişli offset hesabıyla üretilir; IFD0, Exif
  IFD, ASCII/RATIONAL değer blokları ve gömülü önizleme içerir.

### Kapsanan kenar durumları

| Alan | Testler |
|---|---|
| IFD varyasyonları | küçük/büyük endian, satır içi/satır dışı değer, `RATIONAL`, boş dizin, 8 bayttan kısa dosya |
| Bozuk TIFF | geçersiz byte sırası imzası, bilinmeyen alan tipi, kapsam dışı offset, girdi sayısı sınırı, 8 KiB değer sınırı |
| Bozuk EXIF | sıfır paydalı RATIONAL, aralık dışı tarih bileşenleri, NUL sonrası çöp |
| BigTIFF reddi | `II+` imzası |
| Sonsuz döngü koruması | ziyaret kümesi (aynı offset iki kez), `IFD_ADIM_LIMITI` |
| EXIF tarih ayrıştırma | 14 bozuk biçim reddi, ISO 8601 çevrimi, gün sınırı, saniye farkı |
| Farklı kameralar | Canon / Nikon / Sony (90° döndürülmüş yön kodu) |
| JPEG önizleme çıkarma | `JPEGInterchangeFormat`, `StripOffsets` yedeği, sıfır uzunluk, kapsam dışı offset, bellek sınırı, parça parça yazma |
| Önizleme yok | `OnizlemeYok` hatası, raporda tek üyelik grup |
| Bozuk JPEG | SOI yok, progressive `SOF2`, kayıpsız `SOF3`, boyut sınırı, entropi verisi yarım, `DQT` sıfır katsayı, beklemeden marker |
| Perceptual hash | aynı görüntü → aynı hash, düz görüntü → sıfır hash, parlaklık ofseti hash'i değiştirmez, dikey/yatay ayrımı, EPS toleransı |
| DCT doğruluğu | tablo ↔ `cos`, iki geçişli uygulama ↔ naif O(n⁴) referans, DC ölçekleme, Parseval, bilinen 1-D ana köşegen vektörü |
| Hamming eşiği | sınırda birleşir, sınırı aşınca ayrılır, eşik 0, eşik 64, önek hızlandırması ≡ tam tarama |
| Grup oluşturma | tek üye, ikiz kareler, 50 karelik burst, 20 karelik dizi |
| Zincir birleşme | A~B, B~C, A!~C → tek grup + çakışma kaydı |
| Puanlama sıralaması | hız/ISO/odak/en-boy monotonluğu, yöne göre kadraj, eksik veri, 0/negatif alanlar, 0-100 aralığı |
| Boş klasör | tarama, rapor, JSON serileştirme |
| Bozuk dosya atlanıyor | tarama sürer, `AtlananDosya` listesine girer, istatistikte sayılır |
| Uzantı filtresi | 23 RAW/TIFF/JPEG uzantısı kabul, `.txt`/`.pdf` reddedilir |
| Toplu dışa aktarma | tüm gruplar, `--grup` filtresi, kaynak klasör değişmezliği, hedef çakışması reddi, ad çakışması numaralandırması, geçici dosya bırakılmaması |
| JSON şema | alan varlığı, gidiş-dönüş, `sema` sürümü, öneri = en yüksek puanlı üye |
| Türkçe yol | boşluk ve `ç/ğ/ü` içeren klasör yolu |

---

## Proje Yapısı

```text
05-burstjudge/
├── Cargo.toml                 # serde + serde_json + clap (yalnız bu üçü)
├── Cargo.lock
├── LICENSE.txt                # MIT, 2026
├── README.md
├── .gitignore
├── examples/
│   └── ornek_klasor.rs        # demo burst klasörü üreticisi (98 satır)
├── src/
│   ├── lib.rs                 # çekirdek API + katman haritası
│   ├── main.rs                # CLI: group / judge / export / info
│   ├── hata.rs                # Hata enum + Display/Error
│   ├── tiff.rs                # TIFF/EXIF IFD ayrıştırıcı
│   ├── meta.rs                # EXIF anlamsal çözümü (tarih, sayılar, yön)
│   ├── onizleme.rs            # gömülü JPEG önizleme çıkarımı
│   ├── jpeg.rs                # baseline JPEG luma kod çözücü
│   ├── jpeg_test_veri.rs      # JPEG kodlayıcı (test vektörleri)
│   ├── bhash.rs               # DCT + algısal hash
│   ├── grupla.rs              # union-find gruplama
│   ├── puan.rs                # metadata puanlama
│   ├── gezgin.rs              # özyinelemeli dizin gezgini
│   ├── motor.rs               # tarama motoru
│   ├── rapor.rs               # JSON rapor şeması
│   ├── aktar.rs               # kaynağa dokunmayan dışa aktarma
│   └── ornek_veri.rs          # TIFF/EXIF dosya üreticisi
└── tests/
    ├── yardimci/mod.rs        # geçici dizin yöneticisi (Drop ile temizlik)
    ├── tarama_entegrasyon.rs  # dosya sistemi üzerinden uçtan uca
    └── cli_entegrasyon.rs     # gerçek ikilinin çağrıldığı testler
```

Satır sayıları (Rust, `target/` hariç): **9 071**.

---

## Yapılandırma

Her ayar ya komut satırından ya da JSON dosyasından gelir. `--ayar` verilmezse
`Ayarlar::varsayilan()` kullanılır.

### Komut satırı bayrakları

| Bayrak | Komut | Varsayılan | Etkisi |
|---|---|---|---|
| `--esik <0-64>` | `group`, `judge` | `8` | Hamming mesafesi eşiği. 0 = yalnız birebir aynı hash. 64 = her şey tek grup. 0-64 dışı reddedilir. |
| `--cikti <yol>` | `group`, `judge` | — | JSON raporun yazılacağı dosya. `judge` için verilmezse JSON stdout'a gider. |
| `--hash-yok` | `group` | kapalı | Algısal hash üretilmez, önizleme okunmaz; yalnız metadatası taranır. |
| `--ayar <yol>` | `judge` | — | Ağırlık/eşik JSON dosyası. Geçersizse hata verilir. |
| `--grup <kimlik>` | `export` | — | Yalnız bu grup dışa aktarılır. Verilmezse tüm gruplar. |
| `--onizleme-yaz <yol>` | `info` | — | Gömülü önizlemeyi ayrı `.jpg` olarak yazar. |

### `config/ayar.json` şeması

```json
{
  "agirlik_keskinlik": 0.30,
  "agirlik_pozlama": 0.25,
  "agirlik_kirpma": 0.10,
  "agirlik_kompozisyon": 0.20,
  "agirlik_iso": 0.15,
  "iso_yeterli": 1600.0,
  "hedef_en_boy": 1.5,
  "kompozisyon_tolere": 0.35,
  "keskin_min_poz": 0.004
}
```

| Alan | Varsayılan | Doğrulama | Etkisi |
|---|---|---|---|
| `agirlik_keskinlik` | `0.30` | negatif/sonsuz reddedilir | Toplamdaki keskinlik payı. Toplam ağırlık 0 olamaz. |
| `agirlik_pozlama` | `0.25` | aynı | Toplamdaki pozlama payı. |
| `agirlik_kirpma` | `0.10` | aynı | Toplamdaki kırpma (kadraja alınmışlık) payı. |
| `agirlik_kompozisyon` | `0.20` | aynı | Toplamdaki kompozisyon payı. |
| `agirlik_iso` | `0.15` | aynı | Toplamdaki ISO dengesi payı. |
| `iso_yeterli` | `1600.0` | pozitif olmalı | "Yeterli ışık" eşiği; puanlamada referans noktası. |
| `hedef_en_boy` | `1.5` | pozitif ve sonlu olmalı | Kompozisyonun hedeflediği en-boy oranı (3:2). |
| `kompozisyon_tolere` | `0.35` | — | En-boy sapmasının hangi değerinde puanın üçte birine düştüğü (logaritmik). |
| `keskin_min_poz` | `0.004` | — | "Yeterince uzun" poz süresi (1/250 s). |

Eksik alanlar `#[serde(default)]` ile varsayılana düşer:

```text
hata: ayar dosyası geçersiz (C:\...\ayar.json): ağırlıklar negatif veya sonlu değil
```

### Sabitler (kod içinde, `pub`)

| Sabit | Değer | Nerede | Amaç |
|---|---|---|---|
| `IFD_GIRD_LIMITI` | 256 | `tiff` | Tek IFD'deki azami girdi. |
| `IFD_ADIM_LIMITI` | 32 | `tiff` | Bir dosyada çözülecek azami IFD adımı (döngü koruması). |
| `DEGER_LIMITI` | 8 KiB | `tiff` | Tek etiket değerinin azami uzunluğu. |
| `PARCA_BOYUTU` | 4 KiB | `tiff` | Dizin okunurken tek `read` çağrısının boyutu. |
| `ONIZLEME_BELLEK_LIMITI` | 24 MiB | `motor` | Tek önizlemenin bellekteki azami boyutu. |
| `YAZMA_PARCA` | 64 KiB | `onizleme` | Önizleme kopyalama tamponu. |
| `MAKS_BOYUT` | 8192 | `jpeg` | JPEG kenar üst sınırı (piksel). |
| `MAKS_PIKSEL` | 64 Mpx | `jpeg` | JPEG toplam piksel üst sınırı. |
| `KOPYA_TAMPON` | 64 KiB | `aktar` | Dışa aktarma kopyalama tamponu. |
| `EPS` | `1e-9` | `bhash` | Hash eşik toleransı. |

---

## Bilinen Sınırlamalar

Bu bölüm kasıtlı olarak dürüsttür. Aşağıdakilerin çoğu MANIFEST kartı 05'te
**ertelenen** olarak listelenmiştir.

### Rapordan sapma

- **LibRaw, Exiv2 ve OpenCV yerine saf Rust uygulama.** Raporda önerilen
  LibRaw'ın gerekçesi "300+ RAW varyantının elle yazılamaması"dır ve bu
  gerekçe doğrudur. Ancak LibRaw'a yalnızca **kısmi çözüm ve gömülü önizleme**
  için ihtiyaç duyulduğu ve gömülü önizleme dosya içinde **ayrı bir JPEG bayt
  dizisi** olduğu için, bu MVP'de önizleme bayt düzeyinde çıkarılır ve JPEG
  kodu elle yazılmıştır.
- **Tam çözüm, demosaic, renk uzayı dönüşümü ve kırpma yok.** Görüntü yalnız
  **luma (Y)** kanalından çözülür; kroma katsayıları entropi akışını ilerletmek
  için çözülür ama piksele dönüştürülmez.

### Ertelenen özellikler (MANIFEST kartı 05 "Ertelenen")

- **Görüntü içeriğine dayalı netlik/hareket puanlaması** — puanlama tamamen
  metadataladır (MANIFEST madde 4). "Varyans of Laplacian" gibi bir ölçüm
  yapılmaz. Rapordan gelen özgün öneri budur ve MVP'de yoktur.
- **Yüz tespiti** — hiçbir model dosyası okunmaz, gömülü yoktur.
- **dHash** — yerine DCT tabanlı hash kullanılır (rapor `b07` şemasıyla
  uyumlu). Klasik dHash (komşu piksel farkı) uygulanmaz.
- **Seçim dışa aktarma şablonu** — `export` kopyalar ama ad şablonu, üst
  klasör oluşturma veya seçim notu yan dosyası üretmez.
- **Önizleme karşılaştırma yüzeyi** — gruplar arası görsel karşılaştırma yoktur.

### Puanlamanın ölçülebilirliği

Raporun kendisi uyarır: *"Puanlama tamamen ölçülmemiştir ve ölçülebilir bir 'en
iyi kare' tanımı değildir."* BurstJudge bu uyarıyı paylaşır:

- **Bileşen ağırlıkları gözle seçilmiştir**, kalibre edilmemiştir.
- **Kırpma** bileşeni gerçek kadraja alınmayı ölçmez; odak uzaklığından
  türetilen bir **yaklaşımdır** (uzun odak → yüksek puan).
- **Kompozisyon** bileşeni yalnız en-boy oranını ve yön kodunu kullanır; estetik
  tercihi yansıtmaz.
- **Kasıtlı karanlık çekimler** (silüet) düşük "pozlama" puanı alır; bu bir
  ölçüm değil, bir yan etkidir. `ExposureBiasValue` yalnız gerekçe metnine
  yansır.
- **Öneri "seç" değil, "öner"dir.** Her grupta en yüksek toplam puanlı kare
  önerilir; puan bileşenleri ve gerekçeleri raporda görünür kalır.

### Biçim kapsamı

- **Yalnız TIFF-tabanlı dosyalar:** CR2, CR3, NEF, NRW, ARW, SRF, SR2, RAF,
  ORF, DNG, PEF, RAW, RWL, 3FR, ERF, MRW, MOS, X3F, SRW, TIF, TIFF, JPG, JPEG.
- **BigTIFF desteklenmiyor** ve bu durum sessizce geçilmiyor: raporun
  `atlanan` listesinde `bigtiff` etiketiyle görünür.
- **TIFF sürüm 42 dışındaki tüm sürümler reddedilir** (Sailing / TIFF/EP
  sürüm 1'in 10. bayttaki "ilk IFD offset'i" alanı bu MVP'de okunmaz).
- **Baseline JPEG'de `SOF2` (progressive), `SOF3` (kayıpsız), aritmetik kodlama
  ve hiyerarşik JPEG reddedilir.** Daha önce JPEG çoğu DSLR'da baseline'dır,
  ancak bazı gövdeler progressive önizleme yazar; bu dosyalar "önizleme
  bozuk" olarak raporlanır, dosya ise atlanmaz.
- **CMYK JPEG, 12-bit örnek derinliği ve kayıpsız (lossless) JPEG yok.**
- **TIFF/EP çok sayfalı ve birleşik görüntü (sub-IFD) yapıları yoktur.**
- **GPS dizini çözülür ama konum verisi rapora taşınmaz** (gizlilik).

### Hash'in sınırları

- **8×8 kutu ortalaması yüksek frekanslı detayı siler.** 8 piksel periyotlu
  bir desen ortalama alındığında düzleşir ve hash `0000000000000000` çıkar.
  Bu, algoritmanın doğal davranışıdır ve demo üreticisi buna göre seçilmiştir.
- **Aynı kompozisyon farklı pozlama ile çekildiğinde** hash genellikle
  değişmez (DC hariç tutulduğu için) — bu istenen davranıştır.
- **Aynı ortalama parlaklığa sahip farklı kompozisyonlar** (ör. solda açık /
  sağda koyu ile solda koyu / sağda açık) DCT sonrası ayrışır, çünkü işaret
  bilgisi AC katsayılarında korunur.

### Tarama

- **Tarama tek iş parçacığıdır.** Paralellik kasıtlı olarak yoktur: raporun
  "tepe bellek dosya sayısıyla artmaz" sözleşmesini bozmamak için.
- **Sembolik bağlantılara inilmez** (döngü koruması); bağlantı üzerindeki
  dosyalar taranmaz.
- **`.git`, `node_modules`, `target`, `.cache`, `$RECYCLE.BIN` atlanır.**
- **Sıralama garantisi yoktur.** `read_dir` sırası platforma göre değişir;
  grup kimlikleri bu sıraya bağlıdır. `Cargo.lock` da üretilip commit edilir.

### `#[allow]` ve `forbid` durumu

- `src/lib.rs` ve `src/main.rs` dosyalarının başında `#![forbid(unsafe_code)]`
  vardır; hiçbir yerde `unsafe` kullanılmaz.
- `clippy::unwrap_used` / `clippy::expect_used` **üretim kodunda açıktır**
  (`#![warn]`), yalnız `#[cfg(test)]` modüllerinde
  `#[allow(clippy::unwrap_used, clippy::expect_used)]` ile gerekçeli olarak
  kapatılır.
- `tests/yardimci/mod.rs` dosyasında `#![allow(dead_code)]` vardır: iki entegrasyon
  test dosyası modülü ayrı ayrı derler, her biri tüm yardımcıları görmez.
- `examples/ornek_klasor.rs` `std::process::exit(1)` kullanır; bu bir
  `examples/` betiğidir, kütüphane kodu değildir.

---

## Gelecek Geliştirmeler

MANIFEST kartı 05'te ertelenenler ve doğal sonraki adımlar:

1. **Daha fazla üretici varyantı** — Fuji (X-T, RAF'tan farklı yerleşim),
   Leaf, Phase One, Kodak ve Pentax dizin düzenleri; her biri `ornek_veri`
   kalıbıyla üretilip teste dönüştürülebilir.
2. **Progressive JPEG çözücü** (`SOF2`) — tarama sırasındaki en sık "bozuk"
   etiketinin gerçek nedeni bu.
3. **CMYK / Adobe APP14** önizleme desteği.
4. **Önceki/sonraki IFD (IFD1) desteği** — küçültülmüş ikinci görüntü birçok
   gövdede IFD1'de durur.
5. **Görüntü içeriğine dayalı netlik ölçümü** — küçültülmüş önizleme üzerinde
   "varyans of Laplacian benzeri" ölçüm; MANIFEST kartı bunu ertelemiştir
   ama metadata puanlamasının en zayıf halkası budur.
6. **Yüz tespiti** — kullanıcının sağladığı model dosyasıyla, model yoksa
   özellik sessizce devre dışı kalacak biçimde.
7. **Seçim dışa aktarma şablonu** — `{{grup}}`, `{{sira}}`, `{{zaman}}`
   yer tutucuları ve atomik yazma.
8. **Önizleme karşılaştırma yüzeyi** — terminal içi ya da statik HTML/SVG çıktı
   (rapor `b05` v1 maddesi).
9. **Paralel tarama** — iş parçacığı sayısı sabit bir üst sınırla (örn. 2)
   sınırlanarak bellek sözleşmesi korunabilir.
10. **Oturum kalıcılığı** — `oturum.json` ile grup ve seçim durumu.

---

## Troubleshooting

### 1) `hata: parametre hatası (esik): hamming mesafesi 0-64 aralığında olmalı`

**Belirti:** `group --esik 99` ile komut hata kodu 1 ile kapanır.

**Neden:** Hamming mesafesi 64 bitlik hash üzerinde tanımlıdır; 64'ten büyük
eşik "her şey aynı" anlamına gelmez, geçersizdir.

**Çözüm:** 0-64 arasında bir değer verin. Kareleri çok sıkı gruplamak için
`--esik 4`, gevşek gruplamak için `--esik 14` deneyin. `gruplar[].cakismali`
alanı `true` ise eşik çok büyüktür.

### 2) `hata: TIFF-tabanlı değil (...): bayt sırası imzası geçersiz: 62 75`

**Belirti:** Dosya atlanır, `atlanan` listesine `tiff-bazli-degil` etiketiyle
girer, tarama sürer.

**Neden:** Dosya `II` (0x49 0x49) veya `MM` (0x4D 0x4D) ile başlamıyor. Dosya
gerçek RAW değildir (metin, PDF, kısmi kopyalanmış dosya) ya da JPEG'dir.

**Çözüm:** Uzantıyı kontrol edin. Dosyayı `burstjudge info` ile deneyin; JPEG
veya desteklenmeyen bir varyant ise önce bir katalog yazılımıyla dönüştürün.
Tarama durmaz, diğer dosyalar işlenir.

### 3) `hata: dışa aktarma hedefi kaynak klasörün içinde olamaz: ...`

**Belirti:** `export` komutu hiçbir dosya kopyalamadan hata kodu 1 ile kapanır.

**Neden:** Hedef, kaynak klasörle aynı ya da kaynağın alt/üst klasörü.
Program kendi kopyasını dışa aktarmaya çalışabilirdi.

**Çözüm:** Farklı bir dizin seçin. Hedefin kaynağın alt klasörü olmaması
gerekir; komut satırında tam yol verin.

### 4) `hash : -` (hash üretilmedi) ama `önizleme : var`

**Belirti:** Önizleme bulunur ve yazılabilir, ancak hash boştur.

**Neden:** Önizleme **progressive JPEG** (`SOF2`) veya kayıpsız/aritmetik
kodlama ise çözücü onu reddeder. Dosya atlanmaz; `onizleme_bozuk` sayacı
artar.

**Çözüm:** `burstjudge info <dosya> --onizleme-yaz oniz.jpg` ile önizlemeyi
çıkarıp bir görüntüleyicide açın. Üretici ayarında önizlemeyi "baseline"a
çevirmek çoğu zaman mümkündür.

### 5) `okunan bayt ... (%95)` — oran beklenenden yüksek

**Belirti:** Demo gibi küçük dosyalarda okunan oran yüksek çıkıyor.

**Neden:** Oran **dosya boyutuna bağlıdır**. 5 KiB'lık bir dosyada dizin
+önizleme zaten dosyanın büyük kısmıdır. Gerçek RAW dosyalarında (20-40 MiB)
bu oran %0,1'in altındadır.

**Çözüm:** `rapor.json` içindeki `istatistik.toplam_dosya_boyutu` değerine
bakın. 2 MiB'den küçük test dosyaları gerçekçi değildir; `examples/ornek_klasor.rs`
her kareyi 2 MiB'e şişirerek gerçekçi oranlar üretir.

---

## Atıflar

### Spesifikasyonlar

- **TIFF 6.0 (Image File Format)** — IFD başlığı, 12 baytlık dizin girdileri,
  alan tipleri, byte sırası. `src/tiff.rs` bu belgeye göre uygulanmıştır.
  <https://www.itu.int/rec/T-REC-TIFF>
- **Exif 2.3 (CIPA DC-008-Translation-2019)** — `ExifIFDPointer` (0x8769),
  `DateTimeOriginal` (0x9003), `ExposureTime` (0x829A), `FNumber` (0x829D),
  `ISOSpeedRatings` (0x8827), `FocalLength` (0x920A), `Orientation` (0x0112),
  `PixelXDimension`/`PixelYDimension` (0xA002/0xA003) ve tarih biçimi
  `YYYY:MM:DD HH:MM:SS`. <https://www.cipa.jp/std/documents/e/DC-008-Translation-2019-E.pdf>
- **TIFF/EP (ISO 12639:2009)** — BigTIFF (sürüm 43) imzası `II+` / `MM+`;
  bu MVP'de bilinçli olarak reddedilir. <https://www.iso.org/standard/46781.html>
- **ITU-T T.81 / ISO IEC 10918-1 — JPEG** — `SOF0`/`SOF2` çerçevesi, `DQT`
  zig-zag sırası (B.2.4.1), `DHT` kanonik kodlama (F.2.2.1), "receive and
  extend" işaret kuralı (F.2.2.1), byte stuffing (B.1.1.5), `RSTn` /
  `DRI` (B.2.4.2), 8×8 DCT/IDCT (A.3.3), standart Huffman tabloları (Annex K).
  <https://www.itu.int/rec/T-REC-T.81>
- **JFIF (ITU-T T.81 Annex B)** — `JFIF` APP0 segmenti, SOF bileşen yapısı.
- **Ayrık Parseval / DCT özdeşliği** — `src/bhash.rs` içindeki
  `dct_enerji_korunumu_parseval` testi doğrulamaktadır.

### Algoritma

- **DCT tabanlı algısal hash (pHash ailesi)** — Kang, Y. M., Yu, Y.-S. ve
  Hong, S. (2012), "Similarity and Dissimilarity Measures for Multimedia
  Images", IEEE Transactions on Multimedia, 14(1), 20-33. 8×8 DCT + ortalama
  eşiği yaklaşımı bu aileden esinlenmiştir. DOI:
  <https://doi.org/10.1109/TMM.2011.155>
- **Union-find (disjoint set) yol sıkıştırma + birleşme ağırlığı** — Tarjan,
  R. E. ve van Leeuwen, E. (1984), "Worst-Case Analysis of Set Union
  Algorithms", Journal of the ACM, 31(2), 245-260.
  <https://doi.org/10.1145/7740.7745>
- **Hamming mesafesi** — Hamming, R. W. (1950), "Error detecting and
  correcting codes", Bell System Technical Journal, 26(4), 589-595.
  <https://doi.org/10.1002/j.1538-7305.1950.tb01338.x>
- **Kanonik (kutu) Huffman kodlama** — Annex F, Huffman, D. A. (1952),
  "A method for the construction of minimum redundancy codes", IRE Trans.
  Inf. Theory, 4(3), 104-110. <https://doi.org/10.1109/TIT.1952.1056963>

### Kullanılan ve modellenen açık kaynak projeler

Bu projede **hiçbir dış Rust kütüphanesi kopyalanmamıştır**. Algoritmaların
tamamı yukarıdaki spesifikasyonlardan bağımsız olarak yazılmıştır. Karşılaştırma
için incelenen (kopyalanmayan) projeler:

- **Exiv2** — EXIF etiket sözlüğü ve IFD çözümlemesi için referans.
  <https://github.com/Exiv2/exiv2> (GPL-2.0)
- **LibRaw** — RAW formatı çeşitliliği ve gömülü önizleme yerleşimleri için
  referans. <https://www.libraw.org/> (LGPL-2.1 / CDDL)
- **ImageMagick** — `identify` çıktısındaki EXIF alan adları için referans.
  <https://imagemagick.org/> (ImageMagick License)
- **pHash (pHash.org)** — algısal hash ailenin karşılaştırma tabanı.
  <http://www.phash.org/>

### Kullanılan Rust crate'leri

- `serde` (MIT/Apache-2.0) — <https://serde.rs/>
- `serde_json` (MIT/Apache-2.0) — <https://github.com/serde-rs/json>
- `clap` (MIT/Apache-2.0) — <https://docs.rs/clap/>
- `proc-macro2`, `quote`, `syn` (clap'in türev makrolarının geçişli bağımlılıkları)

`cargo tree --depth 1` çıktısı yalnız bu üçünü gösterir:

```text
burstjudge v0.1.0 (%USERPROFILE%\Desktop\Projeler\projects\05-burstjudge)
├── clap v4.6.7
├── serde v1.0.229
└── serde_json v1.0.151
```

### Rapor dosyası

Tasarımın kaynağı: `%USERPROFILE%\Desktop\Fikirler\05-atis-triaji.html`
(bölümler `b01` yönetici özeti, `b03` kullanım senaryoları, `b05` özellik
matrisi, `b07` teknik tasarım, `b08` bellek bütçesi, `b09` taşınabilirlik,
`b16` açık sorular). Bu bir yerel dosyadır, URL'si yoktur.

### Rust standart kütüphanesi

- Rust standart kütüphane belgeleri — <https://doc.rust-lang.org/std/>
- Rust 2021 edition rehberi — <https://doc.rust-lang.org/edition-guide/edition-2021/>
- `cargo` yerel rehberi — <https://doc.rust-lang.org/cargo/>

---

## Lisans

MIT lisansı. Tam metin için bkz. [`LICENSE.txt`](LICENSE.txt).

Copyright (c) 2026 BurstJudge contributors.
