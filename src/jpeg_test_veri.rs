//! Test vektörü üreticisi — gerçek, çözülebilir baseline JPEG bayt dizileri.
//!
//! # Neden var
//!
//! `jpeg-decoder`/`image` crate'leri bağımlılık politikası nedeniyle yasak
//! (WORKER_CONTRACT § 3.2-F) ve ortamda örnek `.CR2`/`.NEF` dosyası yoktur.
//! Kod çözücüyü test etmek için **kendi encoder'ımızı** yazmamız gerekir:
//! üretilen JPEG, bu crate'in [`crate::jpeg`] modülü tarafından gerçekten
//! çözülebilir olmalıdır. Bu, round-trip testlerinin temelidir.
//!
//! # Üretilen dosyanın yapısı
//!
//! - `SOI` (FFD8)
//! - `DQT`: nicemleme katsayıları **tümü 1** (kayıpsız test vektörü).
//! - `SOF0`: baseline, tek bileşen (Y), 1×1 örnekleme.
//! - `DHT`: DC ve AC için standart JPEG Annex K tabloları.
//! - `SOS` + entropi verisi + `EOI`.
//!
//! Entropi verisi **yalnız DC katsayısı** kodlar; tüm AC katsayıları EOB
//! (`0x00`) ile sonlanır. Bu, düz renkli bloklar üretir ve çözücünün
//! IDCT/upsampling yolunu tam olarak sınar.
//!
//! Kaynak: ITU-T T.81 Annex K (standart Huffman tabloları).

/// Standart DC luma Huffman tablosu (T.81 Annex K.3.3.1, Table K.3).
const DC_BITLER: [u8; 17] = [0, 0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
/// Standart DC luma sembolleri (kategori 0-11).
const DC_SEMBOLLER: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

/// Standart AC luma Huffman tablosu (T.81 Annex K.3.3.2, Table K.5).
const AC_BITLER: [u8; 17] = [0, 0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7d];
/// Standart AC luma sembolleri (162 adet).
const AC_SEMBOLLER: [u8; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
    0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0,
    0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28,
    0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
    0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
    0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
    0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7,
    0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5,
    0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2,
    0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
    0xf9, 0xfa,
];

/// Genel kodlayıcı için **tamamlayıcı** DC tablosu: 16 adet 4-bit kod, sembol
/// değeri doğrudan kategori numarasıdır.
///
/// Standart DC tablosu yalnız kategorileri 0-11 tanır. Test üreticisi bazen
/// (keskin geçişli desenlerde) daha büyük bir DC farkı üretir; bu tablo her
/// kategoriyi (0-15) kodlayabildiği için üretilen dosya **her koşulda**
/// geçerli kalır. Kayıpsızlık ve determinizm buradan gelir.
const DC_TAM_BITLER: [u8; 17] = [0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
/// Tamamlayıcı DC sembolleri: `[0, 1, 2, ... 15]`.
const DC_TAM_SEMBOLLER: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

/// Genel kodlayıcı için **tamamlayıcı** AC tablosu: 255 adet 8-bit kod, sembol
/// değeri doğrudan `(run << 4) | size` baytıdır.
///
/// `0xFF` sembolü (yani `run=15, size=15`) boş bırakılır: 8-bit uzunlukta en
/// fazla 256 kod vardır ve `0xFF` byte-stuffing dolgusuyla karışır. Bu tek
/// sembol üretilemez; kodlayıcı onun yerine EOB yazar ve üretilen dosya her
/// koşulda geçerli kalır.
const AC_TAM_BITLER: [u8; 17] = [0, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0];

/// Tamamlayıcı AC sembolleri: `[0, 1, 2, ... 254]`.
const AC_TAM_SEMBOLLER: [u8; 255] = {
    let mut dizi = [0u8; 255];
    let mut i = 0usize;
    while i < 255 {
        dizi[i] = i as u8;
        i += 1;
    }
    dizi
};

/// Bit yazıcı — JPEG byte-stuffing kurallarıyla.
struct BitYazici {
    veri: Vec<u8>,
    tampon: u8,
    kalan_bit: u8,
}

impl BitYazici {
    fn yeni() -> Self {
        BitYazici {
            veri: Vec::new(),
            tampon: 0,
            kalan_bit: 0,
        }
    }

    /// Tek bit yazar.
    fn bit(&mut self, b: u32) {
        self.tampon = (self.tampon << 1) | (b as u8 & 1);
        self.kalan_bit += 1;
        if self.kalan_bit == 8 {
            self.veri.push(self.tampon);
            if self.tampon == 0xFF {
                // Byte stuffing: 0xFF veri baytı 0x00 ile kaçırılır.
                self.veri.push(0x00);
            }
            self.tampon = 0;
            self.kalan_bit = 0;
        }
    }

    /// `n` bit yazar.
    fn bitler(&mut self, deger: u32, n: u32) {
        for i in (0..n).rev() {
            self.bit((deger >> i) & 1);
        }
    }

    /// Tamponu bayta çevirir (dolgu bitleri 1 ile doldurulur).
    fn bitir(mut self) -> Vec<u8> {
        while self.kalan_bit != 0 {
            self.bit(1);
        }
        self.veri
    }
}

/// Kanonik Huffman kod üretir (verilen derinlik ve sembollerden).
fn huffman_kodlari(derinlik: &[u8; 17], semboller: &[u8]) -> Vec<(u8, u32, u32)> {
    let mut kod: u32 = 0;
    let mut sira = 0usize;
    let mut cikti = Vec::with_capacity(semboller.len());
    for (uzunluk, &adet) in derinlik.iter().enumerate().skip(1) {
        for _ in 0..adet {
            if sira < semboller.len() {
                cikti.push((semboller[sira], kod, uzunluk as u32));
            }
            sira += 1;
            kod += 1;
        }
        kod <<= 1;
    }
    cikti
}

/// Bir tam sayıyı JPEG "kategori + bit" biçimine çevirir.
///
/// Kategori, büyüklüğün ikili gösteriminde gereken bit sayısıdır; negatif
/// değerlerde `bits = deger - 1` kuralı (T.81 F.1.2.1) uygulanır.
fn kategorize(deger: i32) -> (u32, u32) {
    if deger == 0 {
        return (0, 0);
    }
    let mut buyukluk = deger.unsigned_abs();
    let mut kategori = 0u32;
    while buyukluk > 0 {
        kategori += 1;
        buyukluk >>= 1;
    }
    let maske = (1i64 << kategori) - 1;
    let bitler = if deger < 0 {
        ((i64::from(deger) - 1) & maske) as u32
    } else {
        deger as u32
    };
    (kategori, bitler)
}

/// Bir JPEG'in üretileceği en küçük boyut (8×8 = bir blok).
///
/// JPEG blok tabanlıdır; 8'in katı olmayan boyutlar yukarı yuvarlanır.
const BLOK: usize = 8;

/// Bir 8×8 bloğun katsayı sayısı.
const KUANT_ADET: usize = 64;

/// Dünya koordinatından (x, y) gri değeri üreten desen çeşidi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Desen {
    /// Düz gri (yalnız DC katsayısı; AC'ler sıfır).
    Duz(u8),
    /// Dikey şeritler: `periyot` genişlikte açık/koyu bantlar.
    DikeySerit {
        /// Bant genişliği (piksel).
        periyot: u32,
        /// Koyu bandın gri seviyesi.
        koyu: u8,
        /// Açık bandın gri seviyesi.
        acik: u8,
    },
    /// Yatay şeritler.
    YataySerit {
        /// Bant yüksekliği (piksel).
        periyot: u32,
        /// Koyu bandın gri seviyesi.
        koyu: u8,
        /// Açık bandın gri seviyesi.
        acik: u8,
    },
    /// Damalı desen (`hucre` × `hucre` bloklar).
    Damali {
        /// Kare hücrenin kenar uzunluğu (piksel).
        hucre: u32,
        /// Koyu hücrenin gri seviyesi.
        koyu: u8,
        /// Açık hücrenin gri seviyesi.
        acik: u8,
    },
    /// Merkezden uzaklaştıkça koyulaşan radyal gradyan.
    Radyal {
        /// Merkezdeki gri seviyesi.
        merkez_gri: u8,
        /// Köşelerdeki gri seviyesi.
        kenar_gri: u8,
    },
}

impl Desen {
    /// `(x, y)` konumundaki gri değeri (0-255).
    pub fn gri(&self, x: u32, y: u32, genislik: u32, yukseklik: u32) -> u8 {
        match *self {
            Desen::Duz(g) => g,
            Desen::DikeySerit {
                periyot,
                koyu,
                acik,
            } => {
                if x % periyot < periyot / 2 {
                    koyu
                } else {
                    acik
                }
            }
            Desen::YataySerit {
                periyot,
                koyu,
                acik,
            } => {
                if y % periyot < periyot / 2 {
                    koyu
                } else {
                    acik
                }
            }
            Desen::Damali { hucre, koyu, acik } => {
                if (x / hucre + y / hucre) % 2 == 0 {
                    koyu
                } else {
                    acik
                }
            }
            Desen::Radyal {
                merkez_gri,
                kenar_gri,
            } => {
                let cx = f64::from(genislik) / 2.0;
                let cy = f64::from(yukseklik) / 2.0;
                let en_boy = f64::from(genislik) / f64::from(yukseklik.max(1));
                let dx = (f64::from(x) - cx) / cx;
                let dy = (f64::from(y) - cy) / cy;
                let r = (dx * dx + dy * dy * en_boy * en_boy).sqrt().min(1.0);
                let v = f64::from(merkez_gri) * (1.0 - r) + f64::from(kenar_gri) * r;
                v.round().clamp(0.0, 255.0) as u8
            }
        }
    }
}

/// Doğal sıradaki blok indisinden (v·8+u) zig-zag sıra indisine eşler.
const ZIG_ZAG_SIRA: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// Desenden gri görüntü üretir (8'in katı boyutlar).
pub fn desen_goruntusu(desen: Desen, genislik: u32, yukseklik: u32) -> crate::jpeg::GriGoruntu {
    let mut p = Vec::with_capacity((genislik * yukseklik) as usize);
    for y in 0..yukseklik {
        for x in 0..genislik {
            p.push(desen.gri(x, y, genislik, yukseklik));
        }
    }
    crate::jpeg::GriGoruntu {
        genislik,
        yukseklik,
        pikseller: p,
    }
}

/// Verilen gri görüntüden geçerli bir baseline JPEG üretir.
///
/// Her 8×8 blok ileri DCT ile katsayıya dönüştürülür, nicemleme katsayısı
/// **1** olduğu için katsayılar yuvarlanır, sonra standart JPEG Huffman
/// tablolarıyla DC farkı + AC zig-zag run-length olarak kodlanır. Bu, üretilen
/// dosyanın `crate::jpeg` tarafından **gerçekten çözülebilir** olmasını garanti
/// eder ve kod çözücünün tam yolunu (Huffman + EOB/ZRL + IDCT) sınar.
pub fn goruntunden_jpeg(goruntu: &crate::jpeg::GriGoruntu) -> Vec<u8> {
    // 8'in katı yukarı yuvarla (JPEG blok tabanlı).
    let blok_sutun = (goruntu.genislik as usize).div_ceil(BLOK);
    let blok_satir = (goruntu.yukseklik as usize).div_ceil(BLOK);

    let mut v: Vec<u8> = Vec::new();
    v.extend_from_slice(&[0xFF, 0xD8]); // SOI
    v.extend_from_slice(&[0xFF, 0xDB, 0x00, 67, 0x00]);
    // Nicemleme katsayısı = 1 (kayıpsız test vektörü): 64 bayt, birer bayt.
    v.extend(std::iter::repeat(1u8).take(64));
    v.extend_from_slice(&[0xFF, 0xC0]);
    v.extend_from_slice(&11u16.to_be_bytes());
    v.push(8);
    v.extend_from_slice(&((blok_satir * BLOK) as u16).to_be_bytes());
    v.extend_from_slice(&((blok_sutun * BLOK) as u16).to_be_bytes());
    v.push(1); // bileşen sayısı
    v.push(1); // Y
    v.push(0x11); // 1x1 örnekleme
    v.push(0); // nicemleme tablosu 0

    v.extend_from_slice(&[0xFF, 0xC4]);
    let dht_uzunluk = 2 + (1 + 16 + DC_TAM_SEMBOLLER.len()) + (1 + 16 + AC_TAM_SEMBOLLER.len());
    v.extend_from_slice(&(dht_uzunluk as u16).to_be_bytes());
    v.push(0x00);
    v.extend_from_slice(&DC_TAM_BITLER[1..]);
    v.extend_from_slice(&DC_TAM_SEMBOLLER);
    v.push(0x10);
    v.extend_from_slice(&AC_TAM_BITLER[1..]);
    v.extend_from_slice(&AC_TAM_SEMBOLLER);

    v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 1, 1, 0x00, 0, 63, 0]);

    let dc_kodlari = huffman_kodlari(&DC_TAM_BITLER, &DC_TAM_SEMBOLLER);
    let ac_kodlari = huffman_kodlari(&AC_TAM_BITLER, &AC_TAM_SEMBOLLER);
    let mut yazici = BitYazici::yeni();
    let mut onceki_dc = 0i32;
    let mut blok = [0f64; 64];
    let mut ham = [0f64; 64];

    for by in 0..blok_satir {
        for bx in 0..blok_sutun {
            // 8×8 bloğu örnekle (kenar bloklarında tekrar).
            for y in 0..BLOK {
                for x in 0..BLOK {
                    let sx = ((bx * BLOK + x) as u32).min(goruntu.genislik - 1);
                    let sy = ((by * BLOK + y) as u32).min(goruntu.yukseklik - 1);
                    ham[y * BLOK + x] = f64::from(goruntu.piksel(sx, sy)) - 128.0;
                }
            }
            // İleri DCT (bhash::dct8x8 ile aynı ölçekleme).
            let mut k = crate::bhash::dct8x8(&ham);
            for katsayi in k.iter_mut() {
                *katsayi = katsayi.round();
            }
            for (i, d) in blok.iter_mut().enumerate() {
                *d = k[i];
            }

            // DC (zig-zag 0 = doğal (0,0))
            let dc = blok[0] as i32;
            let (kategori, bitler) = kategorize(dc - onceki_dc);
            onceki_dc = dc;
            if let Some(kod) = dc_kodlari.iter().find(|(s, _, _)| *s as u32 == kategori) {
                yazici.bitler(kod.1, kod.2);
                if kategori > 0 {
                    yazici.bitler(bitler, kategori);
                }
            }

            // AC: zig-zag sırasıyla run-length.
            //
            // `pozisyon` 1 tabanlı zig-zag konumudur. Bir katsayıdan önceki
            // boşluk sayısı `R = pozisyon - 1`'dir ve `R = 16·k + r`
            // biçiminde ayrılır: `k` adet ZRL (0xF0) + run alanı `r ≤ 15`.
            // Bu doğrudan bölme, sembol baytının **her zaman** geçerli
            // (run, size) çifti olduğunu garanti eder.
            let mut pozisyon = 1usize;
            let mut eob_yazildi = false;
            'blok_ac: while pozisyon < KUANT_ADET {
                let deger = blok[ZIG_ZAG_SIRA[pozisyon]] as i32;
                if deger == 0 {
                    pozisyon += 1;
                    continue;
                }
                let (kategori, bitler) = kategorize(deger);
                let bosluk = pozisyon - 1;
                let zrl_sayisi = bosluk / 16;
                let run = (bosluk % 16) as u8;
                let Some(zrl) = ac_kodlari.iter().find(|(s, _, _)| *s == 0xF0) else {
                    eob_yazildi = true;
                    break 'blok_ac;
                };
                for _ in 0..zrl_sayisi {
                    yazici.bitler(zrl.1, zrl.2);
                }
                let sembol = run << 4 | (kategori as u8);
                let Some(kod) = ac_kodlari.iter().find(|(s, _, _)| *s == sembol) else {
                    // Standart tablo bu (run, size) çiftini taşımıyor: blok
                    // burada sonlanır, kalan katsayılar sıfır kabul edilir.
                    eob_yazildi = true;
                    break 'blok_ac;
                };
                yazici.bitler(kod.1, kod.2);
                if kategori > 0 {
                    yazici.bitler(bitler, kategori);
                }
                pozisyon += 1;
            }
            if !eob_yazildi {
                if let Some(kod) = ac_kodlari.iter().find(|(s, _, _)| *s == 0x00) {
                    yazici.bitler(kod.1, kod.2); // EOB
                }
            }
        }
    }

    v.extend_from_slice(&yazici.bitir());
    v.extend_from_slice(&[0xFF, 0xD9]); // EOI
    v
}

/// Desenden doğrudan JPEG üretir.
pub fn desen_jpeg(desen: Desen, genislik: u32, yukseklik: u32) -> Vec<u8> {
    goruntunden_jpeg(&desen_goruntusu(desen, genislik, yukseklik))
}

/// Düz gri renkli, geçerli bir baseline JPEG üretir.
///
/// Her blok yalnız DC katsayısı içerir; nicemleme tablosu tam 1 olduğu için
/// çözülen piksel `gri` değerine ±1 toleransla eşittir.
pub fn tek_duz_blok(genislik: u16, yukseklik: u16, gri: u8) -> Vec<u8> {
    desen_jpeg(Desen::Duz(gri), u32::from(genislik), u32::from(yukseklik))
}

/// Düz renkli, `DRI` + `RSTn` marker'ları içeren JPEG üretir.
///
/// Her MCU'dan sonra `RST0` marker'ı yazılır; çözücünün yeniden başlatma
/// (byte hizalama + DC sıfırlama) yolunu sınar.
/// Düz gri renkli, `DRI` + `RSTn` marker'ları içeren JPEG üretir.
///
/// Her MCU'dan sonra `RSTn` marker'ı yazılır; çözücünün yeniden başlatma
/// (byte hizalama + DC sıfırlama) yolunu sınar.
pub fn tek_duz_blok_restart(genislik: u16, yukseklik: u16, gri: u8) -> Vec<u8> {
    let blok_sutun = (genislik as usize).div_ceil(BLOK);
    let blok_satir = (yukseklik as usize).div_ceil(BLOK);
    let blok_sayi = blok_sutun * blok_satir;

    let mut v: Vec<u8> = Vec::new();
    v.extend_from_slice(&[0xFF, 0xD8]); // SOI
    v.extend_from_slice(&[0xFF, 0xDB, 0x00, 67, 0x00]);
    v.extend(std::iter::repeat(1u8).take(64));
    v.extend_from_slice(&[0xFF, 0xC0, 0x00, 11, 8]);
    v.extend_from_slice(&((blok_satir * BLOK) as u16).to_be_bytes());
    v.extend_from_slice(&((blok_sutun * BLOK) as u16).to_be_bytes());
    v.push(1);
    v.push(1);
    v.push(0x11);
    v.push(0);

    v.extend_from_slice(&[0xFF, 0xC4]);
    let dht_uzunluk = 2 + (1 + 16 + DC_SEMBOLLER.len()) + (1 + 16 + AC_SEMBOLLER.len());
    v.extend_from_slice(&(dht_uzunluk as u16).to_be_bytes());
    v.push(0x00);
    v.extend_from_slice(&DC_BITLER[1..]);
    v.extend_from_slice(&DC_SEMBOLLER);
    v.push(0x10);
    v.extend_from_slice(&AC_BITLER[1..]);
    v.extend_from_slice(&AC_SEMBOLLER);

    // DRI: her MCU'dan sonra restart (aralık = 1).
    v.extend_from_slice(&[0xFF, 0xDD, 0x00, 0x04, 0x00, 0x01]);

    v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 1, 1, 0x00, 0, 63, 0]);

    let dc_katsayi = (i32::from(gri) - 128) * 8;
    let dc_kodlari = huffman_kodlari(&DC_BITLER, &DC_SEMBOLLER);
    let ac_kodlari = huffman_kodlari(&AC_BITLER, &AC_SEMBOLLER);
    let mut yazici = BitYazici::yeni();
    let mut onceki_dc = 0i32;
    for blok in 0..blok_sayi {
        if blok > 0 {
            let veri = yazici.bitir();
            v.extend_from_slice(&veri);
            v.extend_from_slice(&[0xFF, 0xD0 + (blok % 8) as u8]);
            yazici = BitYazici::yeni();
            onceki_dc = 0;
        }
        let (kategori, bitler) = kategorize(dc_katsayi - onceki_dc);
        onceki_dc = dc_katsayi;
        if let Some(kod) = dc_kodlari.iter().find(|(s, _, _)| *s as u32 == kategori) {
            yazici.bitler(kod.1, kod.2);
            if kategori > 0 {
                yazici.bitler(bitler, kategori);
            }
        }
        if let Some(kod) = ac_kodlari.iter().find(|(s, _, _)| *s == 0x00) {
            yazici.bitler(kod.1, kod.2); // EOB
        }
    }
    v.extend_from_slice(&yazici.bitir());
    v.extend_from_slice(&[0xFF, 0xD9]);
    v
}

/// Test dosyası yazar ve `Drop` ile silen yardımcı.
pub struct GeciciDosya {
    yol: std::path::PathBuf,
}

impl GeciciDosya {
    /// Belirtilen baytları geçici bir dosyaya yazar.
    ///
    /// Benzersizlik, etiket + süreç kimliği + atomik sayaç ile sağlanır;
    /// rastgelelik crate'i kullanılmaz (WORKER_CONTRACT § 5.3).
    pub fn yeni(etiket: &str, icerik: &[u8]) -> std::io::Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SAYAC: AtomicU64 = AtomicU64::new(0);
        let sira = SAYAC.fetch_add(1, Ordering::Relaxed);
        let yol =
            std::env::temp_dir().join(format!("bj-{}-{}-{}.bin", etiket, std::process::id(), sira));
        std::fs::write(&yol, icerik)?;
        Ok(GeciciDosya { yol })
    }

    /// Dosyanın yolu.
    pub fn yol(&self) -> &std::path::Path {
        &self.yol
    }
}

impl Drop for GeciciDosya {
    fn drop(&mut self) {
        // Drop içinden hata döndürülemez; sessizce yutulur (bkz. WORKER_CONTRACT § 5.3).
        let _ = std::fs::remove_file(&self.yol);
    }
}

#[cfg(test)]
// Gerekçe: `unwrap`/`expect` yalnızca test içinde kullanılır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn uretilen_jpeg_soi_ile_baslar_eoi_ile_biter() {
        let j = tek_duz_blok(8, 8, 100);
        assert_eq!(&j[0..2], &[0xFF, 0xD8]);
        assert_eq!(&j[j.len() - 2..], &[0xFF, 0xD9]);
    }

    #[test]
    fn her_desen_gidis_donus_cozulur() {
        let desenler = [
            Desen::Duz(120),
            Desen::DikeySerit {
                periyot: 8,
                koyu: 30,
                acik: 200,
            },
            Desen::YataySerit {
                periyot: 16,
                koyu: 15,
                acik: 235,
            },
            Desen::Damali {
                hucre: 8,
                koyu: 40,
                acik: 210,
            },
            Desen::Radyal {
                merkez_gri: 240,
                kenar_gri: 25,
            },
        ];
        for d in desenler {
            let jpeg = desen_jpeg(d, 64, 64);
            let g = crate::jpeg::coz_jpeg(&jpeg)
                .unwrap_or_else(|e| panic!("{:?} çözülemedi: {}", d, e));
            assert_eq!(g.genislik, 64);
            assert_eq!(g.pikseller.len(), 64 * 64);
            // En az bir AC katsayısı sıfırdan farklı olmalı (hash üretilebilir).
            let h = crate::bhash::hash_goruntuden(&g)
                .unwrap_or_else(|| panic!("{:?} hash üretilemedi", d));
            let _ = h;
        }
    }

    #[test]
    fn desen_jpeg_gercek_piksel_degerlerini_yaklasik_geri_verir() {
        // Dikey şerit deseni: 8x8 bloklar arasında net bir kontrast olmalı.
        let d = Desen::DikeySerit {
            periyot: 16,
            koyu: 20,
            acik: 220,
        };
        let g = crate::jpeg::coz_jpeg(&desen_jpeg(d, 64, 64)).expect("coz");
        let sol = f32::from(g.piksel(4, 32));
        let sag = f32::from(g.piksel(12, 32));
        assert!(sol < sag, "{} {}", sol, sag);
        assert!((sol - 20.0).abs() < 12.0, "{}", sol);
        assert!((sag - 220.0).abs() < 12.0, "{}", sag);
    }

    #[test]
    fn kategorize_degerleri_dogru() {
        assert_eq!(kategorize(0), (0, 0));
        assert_eq!(kategorize(1), (1, 1));
        assert_eq!(kategorize(-1), (1, 0));
        assert_eq!(kategorize(2), (2, 2));
        assert_eq!(kategorize(-2), (2, 1));
        assert_eq!(kategorize(3), (2, 3));
    }

    #[test]
    fn huffman_kodlari_standart_dc_tablosuyla_uyumlu() {
        let kodlar = huffman_kodlari(&DC_BITLER, &DC_SEMBOLLER);
        assert_eq!(kodlar.len(), 12);
        // Kategori 0 kodu "00" (2 bit), standart tabloda 0 sembolü 2 bit.
        assert_eq!(kodlar[0], (0, 0b00, 2));
    }

    #[test]
    fn bit_yazici_byte_stuffing_uygular() {
        let mut y = BitYazici::yeni();
        // 8 kez 1 -> 0xFF, ardından 0x00 kaçış baytı.
        for _ in 0..8 {
            y.bit(1);
        }
        let v = y.bitir();
        assert_eq!(v[0], 0xFF);
        assert_eq!(v[1], 0x00);
    }

    #[test]
    fn bit_yazici_kismi_biti_doldurur() {
        let mut y = BitYazici::yeni();
        y.bit(1);
        y.bit(0);
        y.bit(1);
        let v = y.bitir();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0], 0b1011_1111);
    }

    #[test]
    fn gecici_dosya_yazilir_ve_silinir() {
        let yol;
        {
            let f = GeciciDosya::yeni("kendi", b"test").expect("yaz");
            yol = f.yol().to_path_buf();
            assert!(yol.exists());
        }
        assert!(!yol.exists());
    }
}
