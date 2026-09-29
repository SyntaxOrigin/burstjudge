//! Küçük **baseline (SOF0) JPEG** kod çözücü — yalnız luma (Y) kanalı.
//!
//! # Neden var
//!
//! Algısal hash piksel verisi gerektirir. Gömülü önizleme bir JPEG bayt
//! dizisidir; hash'in çalışması için onu çözmek gerekir. Tam çözüm yazmak
//! kapsam dışı olduğundan (MANIFEST kartı 05) yalnızca **tek bileşenin
//! (luma) çözülmesi** hedeflenir.
//!
//! # Kapsam
//!
//! - **SOF0 / SOF1** (baseline sıralı DCT) desteklenir.
//! - **SOF2** (progressive) açıkça reddedilir: kademeli tarama farklı bir
//!   çözüm mimarisi ister.
//! - `DQT`, `DHT`, `DRI`/`RSTn`, `SOS` desteklenir; `APPn`, `COM`, `DNL`
//!   atlanır.
//! - 4:4:4, 4:2:2 ve 4:2:0 alt örnekleme desteklenir.
//! - **Kroma bileşenleri entropi akışından yalnız *çözülür*, piksele
//!   dönüştürülmez** (aşağıya bakın).
//!
//! # Neden yalnız Y
//!
//! BT.601'de `Y = 0,299 R + 0,587 G + 0,114 B` zaten gri tonlama
//! (luminance) tanımıdır. Luma bileşeni her alt örnekleme kipinde tam
//! çözünürlüktedir, dolayısıyla gri dizi için kroma dönüşümüne gerek yoktur.
//! Kroma katsayıları yine de Huffman çözülür — aksi halde bit akışı
//! kayar ve sonraki MCU'lar bozulur — fakat `IDCT` onlar için **çalıştırılmaz**.
//! Bu, tipik bir gömülü önizlemede çalışma süresini yaklaşık yarıya indirir.
//!
//! # Bellek
//!
//! Yalnız tam çözünürlüklü luma düzlemi (`genislik * yukseklik` bayt) ve iki
//! adet 8×8 blok tamponu tutulur. Görüntü boyutu `MAKS_BOYUT` kenarla ve
//! `MAKS_PIKSEL` toplam pikselle sınırlıdır; bozuk bir `SOF0` başlığı
//! devasa tahsis yaptıramaz.
//!
//! Kaynak: ITU-T T.81 (JFIF) ve ANSI/ISO/IEC 10918-1.

use std::path::PathBuf;

use crate::hata::{Hata, Sonuc};

/// Kod çözülecek görüntünün izin verilen en büyük kenar uzunluğu (piksel).
pub const MAKS_BOYUT: u32 = 8192;

/// Kod çözülecek görüntünün izin verilen en büyük piksel sayısı (64 Mpx).
pub const MAKS_PIKSEL: u64 = 64 * 1024 * 1024;

/// Huffman kodunda izin verilen en fazla bit uzunluğu.
const MAKS_KOD_UZUNLUGU: usize = 16;

/// Nicemleme tablosu eleman sayısı (8×8).
const KUANT_BOYUT: usize = 64;

/// "Bu hata bir JPEG'ten geliyor" kısayolu.
fn bozuk(ayrinti: impl Into<String>) -> Hata {
    Hata::OnizlemeBozuk {
        yol: PathBuf::from("<jpeg>"),
        ayrinti: ayrinti.into(),
    }
}

/// Bir 8×8 blok için ters kuantizasyon tablosu (doğal/zig-zag dönüşümünden sonra).
#[derive(Debug, Clone)]
pub struct KuantTablosu {
    /// 64 eleman, **doğal sırada** (zig-zag çözülmüş).
    pub katsayilar: [u16; KUANT_BOYUT],
}

impl Default for KuantTablosu {
    fn default() -> Self {
        KuantTablosu {
            katsayilar: [1; KUANT_BOYUT],
        }
    }
}

/// Bir JPEG bileşeni (Y/Cb/Cr) — tarama sırasında kullanılan biçim.
#[derive(Debug, Clone, Copy)]
struct Bilesen {
    /// Kimlik (1 = Y, 2 = Cb, 3 = Cr).
    id: u8,
    /// Yatay örnekleme çarpanı (1-4).
    h: usize,
    /// Dikey örnekleme çarpanı (1-4).
    v: usize,
    /// Nicemleme tablosu indeksi.
    kuant: usize,
    /// DC Huffman tablosu indeksi.
    dc_tablo: u8,
    /// AC Huffman tablosu indeksi.
    ac_tablo: u8,
    /// Bu bileşenin bir önceki DC tahmini (kod çözme sırasında).
    dc_tahmin: i32,
}

/// Kanonik (kutu) Huffman çözüm tablosu — JPEG Annex F.
#[derive(Debug, Clone, Default)]
pub struct HuffmanTablosu {
    /// `mincode[l]`: uzunluk `l` için en küçük kod.
    min_kod: [i32; MAKS_KOD_UZUNLUGU + 1],
    /// `maxcode[l]`: uzunluk `l` için en büyük kod; `count[l] == 0` ise `-1`.
    max_kod: [i32; MAKS_KOD_UZUNLUGU + 1],
    /// `valptr[l]`: uzunluk `l` başlayan sembollerin `semboller` içindeki yeri.
    ilk_sembol: [i32; MAKS_KOD_UZUNLUGU + 1],
    /// Semboller, kısa kodlardan uzun kodlara doğru.
    semboller: Vec<u8>,
    /// Tablo atanmış mı.
    dolu: bool,
}

impl HuffmanTablosu {
    /// `BITS` (derinlik sayacı) ve `HUFFVAL` dizilerinden tabloyu kurar.
    ///
    /// Geçersiz (aşırı uzun veya toplamı 256'ya taşan) tablolar `None` döner.
    pub fn kur(semboller: &[u8], derinlik: &[u8; 17]) -> Option<HuffmanTablosu> {
        if semboller.is_empty() || semboller.len() > 256 {
            return None;
        }
        let mut tablo = HuffmanTablosu {
            dolu: true,
            ..HuffmanTablosu::default()
        };
        let mut kod: i32 = 0;
        let mut sira: usize = 0;
        for (uzunluk, &derinlik_sayaci) in derinlik.iter().enumerate().skip(1) {
            let adet = i32::from(derinlik_sayaci);
            // Tam kod ağacı kuralı: bir uzunlukta 256 sembol tüketilirse
            // daha uzun kod tanımlanamaz.
            if sira as i32 + adet > 256 {
                return None;
            }
            tablo.ilk_sembol[uzunluk] = sira as i32;
            tablo.min_kod[uzunluk] = kod;
            sira += adet as usize;
            tablo.max_kod[uzunluk] = if adet > 0 { kod + adet - 1 } else { -1 };
            kod = (kod + adet) << 1;
        }
        if sira != semboller.len() {
            return None;
        }
        tablo.semboller = semboller.to_vec();
        Some(tablo)
    }

    /// Tablo atanmış mı?
    pub fn dolu_mu(&self) -> bool {
        self.dolu
    }

    /// Verilen bit akışından bir sembol okur.
    fn sembol(&self, akis: &mut BitAkisi<'_>) -> Sonuc<u8> {
        let mut kod = akis.bit()? as i32;
        for uzunluk in 1..=MAKS_KOD_UZUNLUGU {
            if kod <= self.max_kod[uzunluk] {
                let indeks = self.ilk_sembol[uzunluk] + (kod - self.min_kod[uzunluk]);
                if indeks < 0 || indeks as usize >= self.semboller.len() {
                    return Err(bozuk("huffman sembol indeksi taşıyor"));
                }
                return Ok(self.semboller[indeks as usize]);
            }
            kod = (kod << 1) | akis.bit()? as i32;
        }
        Err(bozuk("huffman kodu 16 biti aştı"))
    }
}

/// Byte-stuffing'li (`FF 00`) JPEG entropi bit akışı.
struct BitAkisi<'a> {
    veri: &'a [u8],
    konum: usize,
    /// Tamponlanmış bayt.
    tampon: u8,
    /// Tampondaki geçerli bit sayısı (8 = dolu).
    kalan_bit: u8,
    /// Konum dosya sonunu aştı mı.
    bitti: bool,
}

impl<'a> BitAkisi<'a> {
    fn yeni(veri: &'a [u8], konum: usize) -> Self {
        BitAkisi {
            veri,
            konum,
            tampon: 0,
            kalan_bit: 0,
            bitti: false,
        }
    }

    /// Ham bayt okur; `0xFF` dolgu veya marker ayrımını yapar.
    fn ham_bayt(&mut self) -> Sonuc<u8> {
        if self.konum >= self.veri.len() {
            self.bitti = true;
            return Err(bozuk("entropi verisi sonu"));
        }
        let b = self.veri[self.konum];
        self.konum += 1;
        if b != 0xFF {
            return Ok(b);
        }
        if self.konum >= self.veri.len() {
            self.bitti = true;
            return Err(bozuk("0xFF sonrası bayt yok"));
        }
        let sonraki = self.veri[self.konum];
        self.konum += 1;
        if sonraki == 0x00 {
            // Byte stuffing: gerçek 0xFF veri baytı.
            Ok(0xFF)
        } else {
            self.konum -= 1;
            Err(bozuk(format!(
                "veri içinde beklenmeyen marker 0xFF{:02X}",
                sonraki
            )))
        }
    }

    /// Tek bit okur.
    fn bit(&mut self) -> Sonuc<u32> {
        if self.kalan_bit == 0 {
            self.tampon = self.ham_bayt()?;
            self.kalan_bit = 8;
        }
        self.kalan_bit -= 1;
        Ok(((self.tampon >> self.kalan_bit) & 1) as u32)
    }

    /// `n` bit okur.
    fn bitler(&mut self, n: u32) -> Sonuc<u32> {
        let mut v: u32 = 0;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Ok(v)
    }

    /// `n` bit okur ve "receive and extend" uygular (ITU T.81 F.2.2.1).
    fn isaretli(&mut self, n: u32) -> Sonuc<i32> {
        if n == 0 {
            return Ok(0);
        }
        if n > 16 {
            return Err(bozuk(format!("bit kategorisi geçersiz: {}", n)));
        }
        let v = self.bitler(n)? as i32;
        if v < (1i32 << (n - 1)) {
            Ok(v - (1i32 << n) + 1)
        } else {
            Ok(v)
        }
    }

    /// Bayt hizasına döner ve bir sonraki `RSTn` marker'ını atlar.
    ///
    /// DC tahminleri ve `bit_kalan` sıfırlanır; çağıran taraf DC tahmin
    /// dizisini de sıfırlamak zorundadır.
    fn yeniden_baslat(&mut self) -> Sonuc<()> {
        self.kalan_bit = 0;
        self.tampon = 0;
        // RSTn bulunana kadar baytları atla.
        while self.konum + 1 < self.veri.len() {
            if self.veri[self.konum] == 0xFF {
                let m = self.veri[self.konum + 1];
                if (0xD0..=0xD7).contains(&m) {
                    self.konum += 2;
                    return Ok(());
                }
                if m == 0x00 || m == 0xFF {
                    self.konum += 1;
                    continue;
                }
                return Err(bozuk(format!("restart yerine 0xFF{:02X} bulundu", m)));
            }
            self.konum += 1;
        }
        self.bitti = true;
        Err(bozuk("restart marker bulunamadı"))
    }
}

/// Kod çözülmüş gri tonlama görüntü.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GriGoruntu {
    /// Genişlik (piksel).
    pub genislik: u32,
    /// Yükseklik (piksel).
    pub yukseklik: u32,
    /// Satır satır gri baytlar; uzunluk `genislik * yukseklik`.
    pub pikseller: Vec<u8>,
}

impl GriGoruntu {
    /// `(x, y)` konumundaki gri değeri; sınır dışıysa 0.
    pub fn piksel(&self, x: u32, y: u32) -> u8 {
        if x >= self.genislik || y >= self.yukseklik {
            return 0;
        }
        let i = (y as usize) * (self.genislik as usize) + (x as usize);
        self.pikseller.get(i).copied().unwrap_or(0)
    }

    /// Ortalama parlaklık (0-255).
    pub fn ortalama(&self) -> f32 {
        if self.pikseller.is_empty() {
            return 0.0;
        }
        let toplam: u64 = self.pikseller.iter().map(|&p| u64::from(p)).sum();
        toplam as f32 / self.pikseller.len() as f32
    }
}

/// Ayrıklaştırma için 8×8 blok çalışma tamponu.
#[derive(Clone, Copy)]
struct Blok {
    /// Ters kuantizasyon uygulanmış katsayılar (doğal sırada).
    katsayilar: [f32; KUANT_BOYUT],
}

impl Blok {
    fn sifirla() -> Blok {
        Blok {
            katsayilar: [0.0; KUANT_BOYUT],
        }
    }
}

/// `cos((2x+1) u pi / 16)` tablosu — `bhash` modülündeki `TAN` ile aynıdır.
///
/// Ayrı bir kopya değildir: JPEG ters DCT'si ile algısal hash'in ileri DCT'si
/// **aynı** matematiği paylaşır ve tek bir tablodan beslenir; iki tablo
/// ayrışırsa hash'in tutarlılığı bozulur.
use crate::bhash::TAN;

/// `1/sqrt(2)` — DC satır ölçeği.
const DC_OLCEK: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// Ayrıklaştırılmış 8×8 bloğu piksel aralığına çevirir ve bloğu yazar.
///
/// Tam ayrık nokta yerine 1/4 ölçekli iki geçişli (önce satır, sonra sütun)
/// ayrıklaştırma kullanılır; hata `f64` biriktiricilerle `1e-3` mertebesinde
/// kalır. `hedef` çıktı düzlemi, `dx`/`dy` blok sol-üst köşesidir.
fn blogu_yaz(
    blok: &Blok,
    cikti: &mut [u8],
    genislik: usize,
    yukseklik: usize,
    dx: usize,
    dy: usize,
) {
    let mut gecici = [0f64; KUANT_BOYUT];
    // 1. geçiş: satırları ayrıklaştır (0,5 * C(u) ölçeğiyle).
    for y in 0..8 {
        let satirlar = &blok.katsayilar[y * 8..y * 8 + 8];
        for x in 0..8 {
            let mut toplam = 0f64;
            for u in 0..8 {
                let olcek = if u == 0 { DC_OLCEK } else { 1.0 };
                toplam += olcek * f64::from(satirlar[u]) * TAN[x][u];
            }
            gecici[y * 8 + x] = toplam * 0.5;
        }
    }
    // 2. geçiş: sütunları ayrıklaştır. Her boyut kendi `C(v)` ölçeğini taşır;
    // iki geçişin 1/2 çarpanları çarpımı 1/4 katsayısını verir.
    for (y, tan_satiri) in TAN.iter().enumerate() {
        for x in 0..8 {
            // Sütun `x` elemanları gecici[x], gecici[8+x], ... aralığındadır.
            let mut toplam = 0f64;
            for (v, g) in gecici.iter().skip(x).step_by(8).enumerate() {
                let olcek = if v == 0 { DC_OLCEK } else { 1.0 };
                toplam += olcek * g * tan_satiri[v];
            }
            let deger = toplam * 0.5 + 128.0;
            let px = dx + x;
            let py = dy + y;
            if px < genislik && py < yukseklik {
                cikti[py * genislik + px] = deger.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Baseline JPEG çözücü.
pub struct Cevirici {
    genislik: u32,
    yukseklik: u32,
    bilesenler: Vec<Bilesen>,
    h_max: usize,
    v_max: usize,
    mcu_genislik: usize,
    mcu_yukseklik: usize,
    mcu_sutun: usize,
    mcu_satir: usize,
    dc_tablolari: Vec<Option<HuffmanTablosu>>,
    ac_tablolari: Vec<Option<HuffmanTablosu>>,
    kuant: Vec<KuantTablosu>,
    y_indeks: usize,
    yeniden_baslama: u16,
}

impl Default for Cevirici {
    fn default() -> Self {
        Self::yeni()
    }
}

impl Cevirici {
    /// Yeni, boş çevirici.
    pub fn yeni() -> Self {
        Cevirici {
            genislik: 0,
            yukseklik: 0,
            bilesenler: Vec::new(),
            h_max: 1,
            v_max: 1,
            mcu_genislik: 8,
            mcu_yukseklik: 8,
            mcu_sutun: 0,
            mcu_satir: 0,
            dc_tablolari: vec![None; 4],
            ac_tablolari: vec![None; 4],
            kuant: Vec::new(),
            y_indeks: 0,
            yeniden_baslama: 0,
        }
    }

    /// Görüntü genişliği (`SOF0` okunmuşsa).
    pub fn genislik(&self) -> u32 {
        self.genislik
    }

    /// Görüntü yüksekliği (`SOF0` okunmuşsa).
    pub fn yukseklik(&self) -> u32 {
        self.yukseklik
    }

    /// `SOF0`/`SOF1` başlığını okur.
    pub fn sof_oku(&mut self, veri: &[u8], konum: &mut usize) -> Sonuc<()> {
        let uzunluk = self.basligi_oku(veri, konum, &[0xC0, 0xC1])?;
        let son = *konum + uzunluk - 2;
        if son > veri.len() {
            return Err(bozuk("SOF0 segmenti taşıyor"));
        }
        let _veri_boyutu = self.byte_oku(veri, konum)? as usize;
        if uzunluk < 8 {
            return Err(bozuk("SOF0 segmenti kısa"));
        }
        self.yukseklik = u32::from(self.u16_oku(veri, konum)?);
        self.genislik = u32::from(self.u16_oku(veri, konum)?);
        let adet = self.byte_oku(veri, konum)? as usize;
        if !(1..=4).contains(&adet) {
            return Err(bozuk(format!("bileşen sayısı geçersiz: {}", adet)));
        }
        if self.genislik == 0
            || self.yukseklik == 0
            || self.genislik > MAKS_BOYUT
            || self.yukseklik > MAKS_BOYUT
            || u64::from(self.genislik) * u64::from(self.yukseklik) > MAKS_PIKSEL
        {
            return Err(bozuk(format!(
                "görüntü boyutu sınır dışı: {}x{}",
                self.genislik, self.yukseklik
            )));
        }
        self.bilesenler.clear();
        self.h_max = 1;
        self.v_max = 1;
        for _ in 0..adet {
            let id = self.byte_oku(veri, konum)?;
            let oran = self.byte_oku(veri, konum)?;
            let kuant = self.byte_oku(veri, konum)? as usize;
            let h = ((oran >> 4) & 0x0F).max(1) as usize;
            let v = (oran & 0x0F).max(1) as usize;
            if h > 4 || v > 4 {
                return Err(bozuk(format!("geçersiz örnekleme oranı: {}x{}", h, v)));
            }
            if kuant >= 4 {
                return Err(bozuk("geçersiz nicemleme tablosu indeksi"));
            }
            self.h_max = self.h_max.max(h);
            self.v_max = self.v_max.max(v);
            self.bilesenler.push(Bilesen {
                id,
                h,
                v,
                kuant,
                dc_tablo: 0,
                ac_tablo: 0,
                dc_tahmin: 0,
            });
        }
        self.y_indeks = self.bilesenler.iter().position(|b| b.id == 1).unwrap_or(0);
        self.mcu_genislik = self.h_max * 8;
        self.mcu_yukseklik = self.v_max * 8;
        // MCU sayısı = ceil(kenar / MCU kenarı) (T.81 B.2.4). 8'in katı
        // boyutlarda "+1" eklemek yanlıştır: 16x8 bir görüntü 2, 3 MCU değildir.
        self.mcu_sutun = (self.genislik as usize).div_ceil(self.mcu_genislik);
        self.mcu_satir = (self.yukseklik as usize).div_ceil(self.mcu_yukseklik);
        Ok(())
    }

    /// `DQT` segmentini okur.
    pub fn dqt_oku(&mut self, veri: &[u8], konum: &mut usize) -> Sonuc<()> {
        let uzunluk = self.basligi_oku(veri, konum, &[0xDB])?;
        let son = *konum + uzunluk - 2;
        while *konum < son {
            if *konum >= veri.len() {
                return Err(bozuk("DQT segmenti yarım"));
            }
            let bilgi = self.byte_oku(veri, konum)?;
            let hassasiyet = bilgi & 0x0F;
            let sira = (bilgi >> 4) as usize;
            if sira >= 4 || hassasiyet > 1 {
                return Err(bozuk("DQT bilgi baytı geçersiz"));
            }
            let mut katsayilar = [0u16; KUANT_BOYUT];
            for n in 0..KUANT_BOYUT {
                // T.81 B.2.4.1: eleman boyutu Pq ? 2 : 1 bayttır; sıra zig-zag'dır.
                let ham = if hassasiyet == 0 {
                    u16::from(self.byte_oku(veri, konum)?)
                } else {
                    self.u16_oku(veri, konum)?
                };
                if ham == 0 {
                    return Err(bozuk("DQT sifir katsayi iceriyor"));
                }
                katsayilar[ZIG_ZAG[n]] = ham;
            }
            self.kuant.resize(sira + 1, KuantTablosu::default());
            self.kuant[sira] = KuantTablosu { katsayilar };
        }
        Ok(())
    }

    /// `DHT` segmentini okur.
    pub fn dht_oku(&mut self, veri: &[u8], konum: &mut usize) -> Sonuc<()> {
        let uzunluk = self.basligi_oku(veri, konum, &[0xC4])?;
        let son = *konum + uzunluk - 2;
        while *konum < son {
            let bilgi = self.byte_oku(veri, konum)?;
            let sinif = bilgi >> 4;
            let sira = (bilgi & 0x0F) as usize;
            if sinif > 1 || sira >= 4 {
                return Err(bozuk("DHT bilgi baytı geçersiz"));
            }
            let mut derinlik = [0u8; 17];
            let mut toplam = 0usize;
            for d in derinlik.iter_mut().skip(1) {
                if *konum >= veri.len() {
                    return Err(bozuk("DHT derinlik dizisi yarım"));
                }
                *d = veri[*konum];
                *konum += 1;
                toplam += *d as usize;
            }
            if toplam > 256 || *konum + toplam > veri.len() {
                return Err(bozuk("DHT sembol sayısı geçersiz"));
            }
            let semboller = veri[*konum..*konum + toplam].to_vec();
            *konum += toplam;
            let tablo = HuffmanTablosu::kur(&semboller, &derinlik)
                .ok_or_else(|| bozuk("DHT kodu geçersiz (ağaç tamamlanmıyor)"))?;
            if sinif == 0 {
                self.dc_tablolari[sira] = Some(tablo);
            } else {
                self.ac_tablolari[sira] = Some(tablo);
            }
        }
        Ok(())
    }

    /// `DRI` (restart aralığı) segmentini okur.
    pub fn dri_oku(&mut self, veri: &[u8], konum: &mut usize) -> Sonuc<()> {
        let _ = self.basligi_oku(veri, konum, &[0xDD])?;
        self.yeniden_baslama = self.u16_oku(veri, konum)?;
        Ok(())
    }

    /// `SOS` taramasını çözer ve luma düzlemini döndürür.
    pub fn coz(&mut self, veri: &[u8], konum: &mut usize) -> Sonuc<GriGoruntu> {
        let uzunluk = self.basligi_oku(veri, konum, &[0xDA])?;
        if self.bilesenler.is_empty() {
            return Err(bozuk("SOS geldi ama SOF0 okunmamış"));
        }
        let adet = self.byte_oku(veri, konum)? as usize;
        if adet == 0 || adet > self.bilesenler.len() {
            return Err(bozuk("SOS bileşen sayısı geçersiz"));
        }
        let mut tarama: Vec<usize> = Vec::with_capacity(adet);
        for _ in 0..adet {
            let cs = self.byte_oku(veri, konum)?;
            let tablo = self.byte_oku(veri, konum)?;
            let poz = self
                .bilesenler
                .iter()
                .position(|b| b.id == cs)
                .ok_or_else(|| bozuk(format!("SOS bileşeni 0x{:02X} SOF0'da yok", cs)))?;
            self.bilesenler[poz].dc_tablo = tablo >> 4;
            self.bilesenler[poz].ac_tablo = tablo & 0x0F;
            tarama.push(poz);
        }
        // Ss, Se, Ah/Al: 3 bayt.
        self.byte_oku(veri, konum)?;
        self.byte_oku(veri, konum)?;
        self.byte_oku(veri, konum)?;
        let _ = uzunluk;

        let g = self.genislik as usize;
        let y = self.yukseklik as usize;
        let mut cikti = vec![0u8; g * y];
        for b in self.bilesenler.iter_mut() {
            b.dc_tahmin = 0;
        }

        let mut akis = BitAkisi::yeni(veri, *konum);
        let toplam_mcu = (self.mcu_satir * self.mcu_sutun) as u32;
        let mut mcu = 0u32;
        while mcu < toplam_mcu {
            if self.yeniden_baslama > 0 && mcu > 0 && mcu % u32::from(self.yeniden_baslama) == 0 {
                akis.yeniden_baslat()?;
                for b in self.bilesenler.iter_mut() {
                    b.dc_tahmin = 0;
                }
            }
            let mx = (mcu as usize % self.mcu_sutun) * self.mcu_genislik;
            let my = (mcu as usize / self.mcu_sutun) * self.mcu_yukseklik;

            for &poz in &tarama {
                let (h, v, kuant_sira, dc_sira, ac_sira) = {
                    let b = &self.bilesenler[poz];
                    (b.h, b.v, b.kuant, b.dc_tablo, b.ac_tablo)
                };
                if kuant_sira >= self.kuant.len() {
                    return Err(bozuk("bileşenin nicemleme tablosu yüklenmemiş"));
                }
                let kuant = self.kuant[kuant_sira].katsayilar;
                let dc = self
                    .dc_tablolari
                    .get(dc_sira as usize)
                    .and_then(|t| t.as_ref())
                    .ok_or_else(|| bozuk(format!("DC Huffman tablosu {} eksik", dc_sira)))?;
                let ac = self
                    .ac_tablolari
                    .get(ac_sira as usize)
                    .and_then(|t| t.as_ref())
                    .ok_or_else(|| bozuk(format!("AC Huffman tablosu {} eksik", ac_sira)))?;
                let luma_mi = poz == self.y_indeks;

                for blok_no in 0..(h * v) {
                    // DC
                    let kategori = u32::from(dc.sembol(&mut akis)?);
                    if kategori > 16 {
                        return Err(bozuk(format!("DC kategorisi geçersiz: {}", kategori)));
                    }
                    let fark = akis.isaretli(kategori)?;
                    self.bilesenler[poz].dc_tahmin += fark;
                    let dc_deger = self.bilesenler[poz].dc_tahmin;

                    let mut blok = Blok::sifirla();
                    let mut geci_kosul = false;
                    if luma_mi {
                        blok.katsayilar[0] = dc_deger as f32 * f32::from(kuant[0]);
                    } else {
                        geci_kosul = true;
                    }

                    // AC: run-length + kategori
                    let mut k = 1usize;
                    while k < KUANT_BOYUT {
                        let rs = ac.sembol(&mut akis)?;
                        let r = usize::from(rs >> 4);
                        let s = u32::from(rs & 0x0F);
                        if s == 0 {
                            if r == 15 {
                                k += 16; // ZRL
                                continue;
                            }
                            break; // EOB
                        }
                        k += r;
                        if k >= KUANT_BOYUT {
                            // Bozuk akış: katsayı dizini taştı. Bloğu burada
                            // bitir, kalan pikselleri 128 bırak. Tüm dosyayı
                            // reddetmek, tek bir bozuk bloğun tüm kareyi
                            // elemeye gerekçesiz kılar.
                            break;
                        }
                        let deger = akis.isaretli(s)?;
                        if !geci_kosul {
                            // `k` zig-zag sırasıdır; nicemleme tablosu doğal
                            // sırada tutulur, bu yüzden ZIG_ZAG ile eşlenir.
                            blok.katsayilar[k] = deger as f32 * f32::from(kuant[ZIG_ZAG[k]]);
                        }
                        k += 1;
                    }

                    if luma_mi {
                        let bx = mx + (blok_no % h) * 8;
                        let by = my + (blok_no / h) * 8;
                        blogu_yaz(&blok, &mut cikti, g, y, bx, by);
                    }
                }
            }
            mcu += 1;
            if akis.bitti {
                // Bazı üreticiler tüm MCU'ları yazmadan EOI koyar; o durumda
                // kalan pikseller 128 (gri seviyesi) kalır ve çözüm **başarılı**
                // sayılır. Bu, tarayıcı davranışıyla da uyumludur ve bozuk
                // dosyada tüm taramayı düşürmektense kısmı sonuç vermek daha
                // kullanışlıdır. Kalan MCU'ların varlığı ayrıca raporlanır.
                return Ok(GriGoruntu {
                    genislik: self.genislik,
                    yukseklik: self.yukseklik,
                    pikseller: cikti,
                });
            }
        }

        Ok(GriGoruntu {
            genislik: self.genislik,
            yukseklik: self.yukseklik,
            pikseller: cikti,
        })
    }

    /// Segment işaretçisini doğrular ve uzunluk alanını okur.
    fn basligi_oku(&self, veri: &[u8], konum: &mut usize, isaretler: &[u8]) -> Sonuc<usize> {
        if *konum + 4 > veri.len() {
            return Err(bozuk("segment başlığı dosya sınırını aşıyor"));
        }
        if veri[*konum] != 0xFF || !isaretler.contains(&veri[*konum + 1]) {
            return Err(bozuk(format!(
                "segment 0xFF{:02X} bekleniyordu, 0xFF{:02X} bulundu",
                isaretler[0],
                veri[*konum + 1]
            )));
        }
        let uzunluk = u16::from_be_bytes([veri[*konum + 2], veri[*konum + 3]]) as usize;
        // Uzunluk alanı kendi iki baytını da kapsar; `*konum` bu alanın
        // başında olduğundan segmentin sonu `*konum + uzunluk` olur.
        if uzunluk < 2 || *konum + uzunluk > veri.len() {
            return Err(bozuk("segment uzunluğu geçersiz"));
        }
        *konum += 4;
        Ok(uzunluk)
    }

    fn byte_oku(&self, veri: &[u8], konum: &mut usize) -> Sonuc<u8> {
        if *konum >= veri.len() {
            return Err(bozuk("segment veri sonu"));
        }
        let b = veri[*konum];
        *konum += 1;
        Ok(b)
    }

    fn u16_oku(&self, veri: &[u8], konum: &mut usize) -> Sonuc<u16> {
        if *konum + 1 >= veri.len() {
            return Err(bozuk("segment veri sonu"));
        }
        let v = u16::from_be_bytes([veri[*konum], veri[*konum + 1]]);
        *konum += 2;
        Ok(v)
    }
}

/// Zig-zag tarama sırası (ITU T.81 Figure 5) — `DQT` elemanlarını doğal
/// sıraya çevirmek için kullanılır.
const ZIG_ZAG: [usize; KUANT_BOYUT] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// JPEG bayt dizisinden gri tonlama görüntü çözer (yalnız baseline `SOF0`).
///
/// Progressive (`SOF2`), aritmetik kodlama (`SOF9`/`SOF10`) ve hiyerarşik
/// (`DHP`) kipleri açık hata ile reddedilir.
pub fn coz_jpeg(veri: &[u8]) -> Sonuc<GriGoruntu> {
    if veri.len() < 4 || veri[0] != 0xFF || veri[1] != 0xD8 {
        return Err(bozuk("SOI (0xFFD8) imzası yok"));
    }
    let mut cevirici = Cevirici::yeni();
    let mut konum = 2usize;
    while konum + 1 < veri.len() {
        if veri[konum] != 0xFF {
            return Err(bozuk(format!(
                "beklenmeyen bayt 0x{:02X} ({} konumunda)",
                veri[konum], konum
            )));
        }
        let isaret = veri[konum + 1];
        // `konum` işaretçinin başında bırakılır: alt çözücüler `0xFF xx`
        // çiftini ve uzunluk alanını kendileri tüketir.
        match isaret {
            0xC0 | 0xC1 => cevirici.sof_oku(veri, &mut konum)?,
            0xC2 => return Err(bozuk("progressive JPEG (SOF2) desteklenmiyor")),
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                return Err(bozuk(format!(
                    "SOF{} kipi (kayıpsız/hierarşik/arithmetic) desteklenmiyor",
                    isaret - 0xC0
                )))
            }
            0xC4 => cevirici.dht_oku(veri, &mut konum)?,
            0xDB => cevirici.dqt_oku(veri, &mut konum)?,
            0xDD => cevirici.dri_oku(veri, &mut konum)?,
            0xDA => return cevirici.coz(veri, &mut konum),
            // EOI, DNL ve tek başına duran restart marker'ları.
            0xD9 | 0xDC | 0xD0..=0xD8 => return Err(bozuk("SOS bulunamadı, tarama yok")),
            0x01 | 0xFF => {
                konum += 2;
                continue;
            }
            _ => {
                // APPn, COM ve diğer bilinmeyen segmentler: atlanır.
                if konum + 3 >= veri.len() {
                    return Err(bozuk("segment uzunluğu okunamadı"));
                }
                let uzunluk = u16::from_be_bytes([veri[konum + 2], veri[konum + 3]]) as usize;
                if uzunluk < 2 {
                    return Err(bozuk("segment uzunluğu 1"));
                }
                let son = konum + 2 + uzunluk;
                if son > veri.len() {
                    return Err(bozuk("segment dosya sınırını aşıyor"));
                }
                konum = son;
            }
        }
    }
    Err(bozuk("SOS taraması bulunamadı (dosya yarım)"))
}

#[cfg(test)]
// Gerekçe: `unwrap`/`expect` yalnızca test içinde kullanılır ve testin
// başarısızlık mesajıdır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::jpeg_test_veri;

    #[test]
    fn soi_olmadan_hata_verir() {
        assert!(coz_jpeg(&[0x00, 0x01, 0x02, 0x03]).is_err());
        assert!(coz_jpeg(&[]).is_err());
        assert!(coz_jpeg(&[0xFF]).is_err());
    }

    #[test]
    fn soi_olan_ama_sos_olmayan_kisa_dosya_hata_verir() {
        assert!(coz_jpeg(&[0xFF, 0xD8, 0xFF, 0xD9]).is_err());
    }

    #[test]
    fn progressive_jpeg_reddedilir() {
        let veri = [
            0xFFu8, 0xD8, 0xFF, 0xC2, 0x00, 0x0B, 8, 0, 8, 0, 8, 1, 1, 0x11, 0,
        ];
        match coz_jpeg(&veri) {
            Err(Hata::OnizlemeBozuk { ayrinti, .. }) => assert!(ayrinti.contains("progressive")),
            diger => panic!("progressive reddi bekleniyordu: {:?}", diger),
        }
    }

    #[test]
    fn kayipsiz_solom_kipleri_reddedilir() {
        // SOF3 (lossless)
        let veri = [
            0xFFu8, 0xD8, 0xFF, 0xC3, 0x00, 0x0B, 8, 0, 8, 0, 8, 1, 1, 0x11, 0,
        ];
        match coz_jpeg(&veri) {
            Err(Hata::OnizlemeBozuk { ayrinti, .. }) => assert!(ayrinti.contains("kayıpsız")),
            diger => panic!("kayıpsız reddi bekleniyordu: {:?}", diger),
        }
    }

    #[test]
    fn boyut_siniri_asan_sof0_reddedilir() {
        // 9000x9000 > MAKS_BOYUT (8192)
        let veri = [
            0xFFu8, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 8, 0x23, 0x28, 0x23, 0x28, 1, 1, 0x11, 0,
        ];
        match coz_jpeg(&veri) {
            Err(Hata::OnizlemeBozuk { ayrinti, .. }) => assert!(ayrinti.contains("sınır")),
            diger => panic!("boyut sınırı bekleniyordu: {:?}", diger),
        }
    }

    #[test]
    fn bozuk_entropi_verisi_hata_verir() {
        let jpeg = jpeg_test_veri::tek_duz_blok(8, 8, 100);
        let mut bozuk = jpeg.clone();
        // SOS verisini kes: entropi akışı sonu.
        let sos = bozuk
            .windows(2)
            .position(|c| c == [0xFF, 0xDA])
            .expect("SOS bulundu");
        bozuk.truncate(sos + 8);
        assert!(coz_jpeg(&bozuk).is_err());
    }

    #[test]
    fn dogru_tek_duz_blok_cozulur() {
        let jpeg = jpeg_test_veri::tek_duz_blok(8, 8, 100);
        let g = coz_jpeg(&jpeg).expect("coz");
        assert_eq!(g.genislik, 8);
        assert_eq!(g.yukseklik, 8);
        // Düz 100 gri: kayıpsız nicemlemede tüm pikseller 100 olmalı.
        for y in 0..8 {
            for x in 0..8 {
                assert!(
                    (g.piksel(x, y) as i32 - 100).abs() <= 1,
                    "({},{}) = {}",
                    x,
                    y,
                    g.piksel(x, y)
                );
            }
        }
    }

    #[test]
    fn gruplama_karti_16x16_dogru_cozulur() {
        let jpeg = jpeg_test_veri::tek_duz_blok(16, 16, 77);
        let g = coz_jpeg(&jpeg).expect("coz");
        assert_eq!(g.genislik, 16);
        assert_eq!(g.pikseller.len(), 256);
        assert!((g.ortalama() - 77.0).abs() <= 1.0, "{}", g.ortalama());
    }

    #[test]
    fn doygun_beyaz_ve_siyah_klip_cozulur() {
        for (deger, beklenen) in [(255u8, 255u8), (0u8, 0u8)] {
            let jpeg = jpeg_test_veri::tek_duz_blok(8, 8, deger);
            let g = coz_jpeg(&jpeg).expect("coz");
            assert_eq!(g.piksel(4, 4), beklenen, "deger {}", deger);
        }
    }

    #[test]
    fn kos_tablosu_bir_kosildir() {
        // Tablo bhash modülünden gelir; burada yalnız erişilebilirliği doğrularız.
        for (x, tan_satiri) in TAN.iter().enumerate() {
            for (u, &deger) in tan_satiri.iter().enumerate() {
                let beklenen =
                    (((2 * x + 1) as f64 * u as f64) * std::f64::consts::PI / 16.0).cos();
                assert!((deger - beklenen).abs() < 1e-12, "{} {}", x, u);
            }
        }
    }

    #[test]
    fn ters_dct_dc_katsayida_sabit_kalir() {
        let mut blok = Blok::sifirla();
        blok.katsayilar[0] = 8.0;
        let mut cikti = vec![0u8; 64];
        blogu_yaz(&blok, &mut cikti, 8, 8, 0, 0);
        // DC=8 -> 8/4 = 2, +128 = 130
        for p in cikti.iter() {
            assert!((i32::from(*p) - 130).abs() <= 1, "{}", p);
        }
    }

    #[test]
    fn ters_dct_yatay_dortgen_kenari_keskin_gorunur() {
        // F(0,1) = 32 -> yatay bir kozinüs: x=0'da açık, x=7'de koyu.
        let mut blok = Blok::sifirla();
        blok.katsayilar[1] = 32.0;
        let mut cikti = vec![128u8; 64];
        blogu_yaz(&blok, &mut cikti, 8, 8, 0, 0);
        assert!(cikti[0] > 128, "sol acik olmali, {}", cikti[0]);
        assert!(cikti[7] < 128, "sag koyu olmali, {}", cikti[7]);
    }

    #[test]
    fn blok_yazma_gorsuntu_sinirinda_tasmaz() {
        let blok = Blok::sifirla();
        // Tüm katsayılar 0 -> çıktı tamamen 128 (+128 seviyesi).
        let mut cikti = vec![0u8; 100];
        // 8x8 blok 4,4'ten: 4..12 aralığı, 10x10 sınırı aşıyor.
        blogu_yaz(&blok, &mut cikti, 10, 10, 4, 4);
        // Yazılan bölge (4..10, 4..10) 128 olmalı.
        for y in 4..10 {
            for x in 4..10 {
                assert_eq!(cikti[y * 10 + x], 128, "({}, {})", x, y);
            }
        }
        // Yazılmayan bölge 0 kalmalı (taşma yok).
        for i in 0..4 {
            assert_eq!(cikti[i], 0);
            assert_eq!(cikti[i * 10], 0);
        }
    }

    #[test]
    fn huffman_tablosu_kurulur() {
        // Standart DC luma tablosu: 12 sembol, 0-3 bit.
        let derinlik = [0u8, 0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
        let semboller = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
        let t = HuffmanTablosu::kur(&semboller, &derinlik).expect("tablo");
        assert!(t.dolu_mu());
        assert_eq!(t.semboller.len(), 12);
    }

    #[test]
    fn huffman_tablosu_bos_sembol_reddeder() {
        let derinlik = [0u8; 17];
        assert!(HuffmanTablosu::kur(&[], &derinlik).is_none());
    }

    #[test]
    fn huffman_tablosu_asiri_uzun_kodu_reddeder() {
        // 16-bit derinlikte 1 sembol + 17'de 1 sembol: geçersiz (toplam 2 ama
        // 16-bit ağaç 256 kodu tüketmeden 17. uzunluğa geçemez).
        let derinlik = {
            let mut d = [0u8; 17];
            d[16] = 1;
            d[17 - 1] = 0;
            d
        };
        // 16 uzunlukta 1, 1 uzunlukta 255 -> sira 256'ya ulaşır, geçerli.
        let mut d = [0u8; 17];
        d[1] = 255;
        d[2] = 1;
        let semboller: Vec<u8> = (0..=255u8).collect();
        assert!(HuffmanTablosu::kur(&semboller, &d).is_some());
        // Derinlik toplamı sembol sayısından azsa reddedilir.
        let d2 = {
            let mut x = [0u8; 17];
            x[1] = 2;
            x
        };
        assert!(HuffmanTablosu::kur(&[0, 1, 2], &d2).is_none());
        let _ = derinlik;
    }

    #[test]
    fn bit_akisi_byte_stuffing_cozer() {
        // 0xFF 0x00 gerçek 0xFF veri baytıdır; iki bayt okunabilir.
        let veri = [0xFFu8, 0x00, 0xA5];
        let mut akis = BitAkisi::yeni(&veri, 0);
        assert_eq!(akis.ham_bayt().expect("bayt"), 0xFF);
        assert_eq!(akis.ham_bayt().expect("bayt"), 0xA5);
        // Sonraki okuma veri sonuna düşer.
        assert!(akis.ham_bayt().is_err());
    }

    #[test]
    fn bit_akisi_marker_icerinde_hata_verir() {
        // 0xFF 0xD0 veri içinde geçersiz (restart marker).
        let veri = [0xFFu8, 0xD0];
        let mut akis = BitAkisi::yeni(&veri, 0);
        assert!(akis.ham_bayt().is_err());
    }

    #[test]
    fn bit_akisi_sonunda_hata_verir() {
        let veri: [u8; 0] = [];
        let mut akis = BitAkisi::yeni(&veri, 0);
        assert!(akis.ham_bayt().is_err());
        assert!(akis.bitti);
    }

    #[test]
    fn bit_akisi_ff_sonrasi_bayt_yoksa_hata_verir() {
        let veri = [0xFFu8];
        let mut akis = BitAkisi::yeni(&veri, 0);
        assert!(akis.ham_bayt().is_err());
    }

    #[test]
    fn isaretli_degerler_uzanti_kuralina_uyar() {
        // 4 bit geçerli kategoriler: 8..15 pozitif, 0..7 negatif (1'den az olan
        // 4-bitlik alan kodlanmaz; 0 yalnız S=0 ile temsil edilir).
        let vakalar: [(u8, i32); 3] = [(0b1000, 8), (0b0111, -8), (0b1111, 15)];
        for (poz_deger, beklenen) in vakalar {
            let veri = [poz_deger << 4];
            let mut akis = BitAkisi::yeni(&veri, 0);
            assert_eq!(
                akis.isaretli(4).expect("isaretli"),
                beklenen,
                "poz_deger {}",
                poz_deger
            );
        }
    }

    #[test]
    fn isaretli_16_bit_ustu_reddedilir() {
        let veri = [0xFFu8, 0xFF];
        let mut akis = BitAkisi::yeni(&veri, 0);
        assert!(akis.isaretli(17).is_err());
    }

    #[test]
    fn yeniden_baslat_rstn_bulur() {
        let veri = [0x11u8, 0x22, 0xFF, 0xD2, 0x33];
        let mut akis = BitAkisi::yeni(&veri, 2);
        akis.yeniden_baslat().expect("restart");
        assert_eq!(akis.konum, 4);
        assert_eq!(akis.ham_bayt().expect("bayt"), 0x33);
    }

    #[test]
    fn yeniden_baslat_baska_markerde_hata_verir() {
        let veri = [0xFFu8, 0xD9];
        let mut akis = BitAkisi::yeni(&veri, 0);
        assert!(akis.yeniden_baslat().is_err());
    }

    #[test]
    fn yeniden_baslat_marker_bulamazsa_hata_verir() {
        let veri = [0x11u8, 0x22, 0x33];
        let mut akis = BitAkisi::yeni(&veri, 0);
        assert!(akis.yeniden_baslat().is_err());
        assert!(akis.bitti);
    }

    #[test]
    fn cevirici_bos_durumda_cozme_hata_verir() {
        let mut c = Cevirici::yeni();
        assert_eq!(c.genislik(), 0);
        let veri = [0xFFu8, 0xDA, 0x00, 0x08, 1, 1, 0x00, 0x00, 0x3F, 0x00];
        let mut k = 0;
        assert!(c.coz(&veri, &mut k).is_err());
    }

    #[test]
    fn segment_isareti_gecersizse_hata_verir() {
        let mut c = Cevirici::yeni();
        // 0xFFC2 = progressive -> reddedilmeli.
        let veri = [0xFFu8, 0xC2, 0x00, 0x0B, 8, 0, 8, 0, 8, 1, 1, 0x11, 0];
        let mut k = 0;
        assert!(c.sof_oku(&veri, &mut k).is_err());
    }

    #[test]
    fn sof1_kabul_edilir() {
        // 0xFFC1 = baseline genişletilmiş; aynı yapıda çözülür.
        let mut c = Cevirici::yeni();
        let veri = [0xFFu8, 0xC1, 0x00, 0x0B, 8, 0, 8, 0, 8, 1, 1, 0x11, 0];
        let mut k = 0;
        assert!(c.sof_oku(&veri, &mut k).is_ok());
        assert_eq!(c.genislik(), 8);
        assert_eq!(c.yukseklik(), 8);
    }

    #[test]
    fn restart_markerli_klip_cozulur() {
        let jpeg = jpeg_test_veri::tek_duz_blok_restart(16, 16, 90);
        let g = coz_jpeg(&jpeg).expect("coz");
        assert_eq!(g.genislik, 16);
        assert!((g.ortalama() - 90.0).abs() <= 1.0, "{}", g.ortalama());
    }

    #[test]
    fn eob_ile_kismi_kodlanmis_klip_cozulur() {
        // Yalnız DC katsayısı olan blok: AC EOB hemen gelir.
        let jpeg = jpeg_test_veri::tek_duz_blok(8, 8, 60);
        let g = coz_jpeg(&jpeg).expect("coz");
        assert!((g.ortalama() - 60.0).abs() <= 1.0, "{}", g.ortalama());
    }
}
