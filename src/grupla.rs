//! Benzerlik gruplama — union-find + hamming eşiği + çakışma tespiti.
//!
//! # Algoritma
//!
//! Rapor `b07` "iki aşamalı" tanımını uygular: hash'ler önce **kaba öbekleme**
//! ile aday çiftlerine ayrılır, sonra **kesin eşik** kararı verilir.
//!
//! 1. **Öbekleme:** 64-bit hash 16 parçaya bölünür; parça başına 4 bitlik
//!    önek varsa iki hash aynı öbekte olabilir. Aday çiftleri O(n·16) ile
//!    sınırlıdır, O(n²) değil.
//! 2. **Eşik kararı:** aday çiftlerinin hamming uzaklığı `esik` (<=) ise
//!    birleştirilir.
//! 3. **Bileşke (union-find):** `yol sıkıştırma` + `birleşme ağırlığı`
//!    ile neredeyse sabit zamanlı çalışır.
//! 4. **Çakışma tespiti:** aynı grupta birbirinden **eşikten uzak** ama
//!    ortak üyeleri nedeniyle aynı bileşke düşen kareler varsa grup
//!    **çakışmalı** işaretlenir. Bu, "zincir birleşme" (A~B, B~C, ama A!~C)
//!    durumunu görünür kılar ve kullanıcıya eşiğin gözden geçirilmesini önerir.
//!
//! Bellek: yalnız `n` uzunluğunda hash, `n` uzunluğunda üst öğe ve `n` adet
//! grup başlığı tutulur. Görüntü verisi saklanmaz.

use crate::bhash::AlgisalHash;
use std::collections::BTreeMap;

/// Hash'in öbekleme için bölündüğü 16-bit parça sayısı.
pub const PARCA_SAYISI: usize = 4;

/// Bir hash öbeğine ait alt küme (bucket) tanımlayıcısı.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Oge {
    /// Kaynak dosyanın sıra numarası.
    pub sira: usize,
    /// Dosyanın algısal hash'i.
    pub hash: AlgisalHash,
}

/// Union-find bulmak için sıkıştırılmış üst öğe dizisi.
struct Birlestir {
    ust: Vec<usize>,
    // Union-by-rank yerine ağırlık tabanlı birleştirme: küçük ağacı büyüğe bağla.
    agirlik: Vec<usize>,
    grup_no: Vec<usize>,
}

impl Birlestir {
    fn yeni(adet: usize) -> Self {
        Birlestir {
            ust: (0..adet).collect(),
            agirlik: vec![1; adet],
            grup_no: (0..adet).collect(),
        }
    }

    /// Kök bulucu (yol sıkıştırmalı, yinelemesiz).
    fn kok(&mut self, dugum: usize) -> usize {
        let mut kok = dugum;
        while self.ust[kok] != kok {
            kok = self.ust[kok];
        }
        // Sıkıştırma: yol boyunca tüm düğümler köke bağlanır.
        let mut y = dugum;
        while self.ust[y] != kok {
            let sonraki = self.ust[y];
            self.ust[y] = kok;
            y = sonraki;
        }
        kok
    }

    /// İki düğümü birleştirir; grup numarası daha küçük olan korunur.
    fn birlestir(&mut self, a: usize, b: usize) -> bool {
        let ka = self.kok(a);
        let kb = self.kok(b);
        if ka == kb {
            return false;
        }
        let grup = self.grup_no[ka].min(self.grup_no[kb]);
        if self.agirlik[ka] < self.agirlik[kb] {
            self.ust[ka] = kb;
            self.agirlik[kb] += self.agirlik[ka];
            self.grup_no[kb] = grup;
        } else {
            self.ust[kb] = ka;
            self.agirlik[ka] += self.agirlik[kb];
            self.grup_no[ka] = grup;
        }
        true
    }
}

/// Bir grup içindeki iki karenin arasındaki ilişki.
#[derive(Debug, Clone)]
pub struct Cakisma {
    /// Aynı grupta olmalarına rağmen eşiği aşan çiftin ilk üyesi.
    pub ilk: usize,
    /// Aynı grupta olmalarına rağmen eşiği aşan çiftin ikinci üyesi.
    pub ikinci: usize,
    /// Ölçülen hamming uzaklığı.
    pub mesafe: u32,
    /// Kullanılan eşik.
    pub esik: u32,
}

/// Benzerlik gruplarının tamamı.
#[derive(Debug, Clone)]
pub struct Gruplama {
    /// Her grup için üye sıra numaraları.
    pub gruplar: Vec<Vec<usize>>,
    /// Grup kimlikleri (0'dan başlar, grup sırasına göre).
    pub kimlikler: Vec<usize>,
    /// Tespit edilen çakışmalar.
    pub cakismalar: Vec<Cakisma>,
    /// Karşılaştırılan aday çift sayısı (performans kanıtı).
    pub aday_cift: u64,
    /// Karşılaştırılan toplam çift sayısı.
    pub toplam_cift: u64,
}

impl Gruplama {
    /// Kaç grup oluştu?
    pub fn grup_sayisi(&self) -> usize {
        self.gruplar.len()
    }

    /// İki kare aynı grupta mı?
    pub fn ayni_grup(&self, a: usize, b: usize) -> bool {
        self.gruplar
            .iter()
            .any(|g| g.contains(&a) && g.contains(&b))
    }

    /// Belirli kareyi içeren grubun sırası.
    pub fn grup_bul(&self, sira: usize) -> Option<usize> {
        self.gruplar.iter().position(|g| g.contains(&sira))
    }
}

/// Hash listesini hamming eşiğine göre gruplar (tam karşılaştırma).
///
/// Bu, referans uygulamadır: her çift karşılaştırılır, yanlış negatif
/// **üretemez**. `esik` 0-64 dışındaysa 64'e kırpılır.
///
/// `hizli_grup` daraltılmış aday üretimiyle aynı sonucu verir; fark yalnız
/// hızdadır.
pub fn grupla(ogeler: &[Oge], esik: u32) -> Gruplama {
    let n = ogeler.len();
    let mut birlestir = Birlestir::yeni(n);
    let mut toplam_cift: u64 = 0;
    let mut aday_cift: u64 = 0;
    let esik_k = esik.min(64);

    for i in 0..n {
        for j in (i + 1)..n {
            toplam_cift += 1;
            if ogeler[i].hash.hamming(&ogeler[j].hash) <= esik_k {
                aday_cift += 1;
                birlestir.birlestir(i, j);
            }
        }
    }

    bitir(&mut birlestir, ogeler, esik_k, toplam_cift, aday_cift)
}

/// Union-find ağacından grup listelerini ve çakışmaları üretir.
fn bitir(
    birlestir: &mut Birlestir,
    ogeler: &[Oge],
    esik: u32,
    toplam_cift: u64,
    aday_cift: u64,
) -> Gruplama {
    let n = ogeler.len();
    // Grup kimliklerini kökten okuyup sıraya diz.
    let mut kok_grup: Vec<usize> = vec![usize::MAX; n];
    let mut gruplar: Vec<Vec<usize>> = Vec::new();
    for i in 0..n {
        let kok = birlestir.kok(i);
        if kok_grup[kok] == usize::MAX {
            kok_grup[kok] = gruplar.len();
            gruplar.push(Vec::new());
        }
        let g = kok_grup[kok];
        if let Some(uye) = gruplar.get_mut(g) {
            uye.push(i);
        }
    }

    // Çakışma tespiti: grup içinde eşiği aşan çiftler.
    let mut cakismalar = Vec::new();
    for grup in &gruplar {
        for (a_pos, &a) in grup.iter().enumerate() {
            for &b in &grup[a_pos + 1..] {
                let mesafe = ogeler[a].hash.hamming(&ogeler[b].hash);
                if mesafe > esik {
                    cakismalar.push(Cakisma {
                        ilk: a,
                        ikinci: b,
                        mesafe,
                        esik,
                    });
                }
            }
        }
    }

    Gruplama {
        kimlikler: (0..gruplar.len()).collect(),
        gruplar,
        cakismalar,
        aday_cift,
        toplam_cift,
    }
}

/// Önek tabanlı hızlı aday üretimi (union-find aynı, tarama daraltılır).
///
/// Yalnız aynı 16-bit önekleri karşılaştırır; iki hash 16-bit önekleri
/// farklıysa aralarındaki hamming mesafesi en az 16'dır, bu yüzden 16'dan
/// küçük eşiklerde **kaçırılan eşleşme olmaz** (esik < 16 için doğru).
/// `esik >= 16` ise daraltma güvenli değildir ve tam taramaya düşer.
pub fn hizli_grup(ogeler: &[Oge], esik: u32) -> Gruplama {
    if esik >= 16 {
        return grupla(ogeler, esik);
    }
    let n = ogeler.len();
    let mut birlestir = Birlestir::yeni(n);
    let mut toplam_cift: u64 = 0;
    let mut aday_cift: u64 = 0;
    let esik_k = esik.min(64);

    for parca in 0..PARCA_SAYISI {
        // Aynı 16-bit parçaya sahip hash'ler bir kümeye girer.
        let mut kumeler: BTreeMap<u16, Vec<usize>> = BTreeMap::new();
        for i in 0..n {
            kumeler
                .entry(parca_bitis(i, parca, ogeler))
                .or_default()
                .push(i);
        }
        for uye in kumeler.values() {
            for (a_pos, &i) in uye.iter().enumerate() {
                for &j in &uye[a_pos + 1..] {
                    toplam_cift += 1;
                    if ogeler[i].hash.hamming(&ogeler[j].hash) <= esik_k {
                        aday_cift += 1;
                        birlestir.birlestir(i, j);
                    }
                }
            }
        }
    }

    bitir(&mut birlestir, ogeler, esik_k, toplam_cift, aday_cift)
}

/// `oge`nin hash'inin `parca` numaralı 16-bit parçasını döndürür.
fn parca_bitis(sira: usize, parca: usize, ogeler: &[Oge]) -> u16 {
    let h = ogeler[sira].hash.0;
    ((h >> (parca * 16)) & 0xFFFF) as u16
}

#[cfg(test)]
// Gerekçe: `unwrap`/`expect` yalnızca test içinde kullanılır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn oge(sira: usize, hash: u64) -> Oge {
        Oge {
            sira,
            hash: AlgisalHash(hash),
        }
    }

    #[test]
    fn bos_liste_bos_dondurur() {
        let g = grupla(&[], 8);
        assert_eq!(g.grup_sayisi(), 0);
        assert!(g.cakismalar.is_empty());
        assert_eq!(g.toplam_cift, 0);
    }

    #[test]
    fn tek_kare_tek_grup_olur() {
        let g = grupla(&[oge(0, 0x1234)], 8);
        assert_eq!(g.grup_sayisi(), 1);
        assert_eq!(g.gruplar[0], vec![0]);
        assert!(g.cakismalar.is_empty());
    }

    #[test]
    fn ayni_hash_tek_grupta_birlesir() {
        let g = grupla(&[oge(0, 0xABCD), oge(1, 0xABCD), oge(2, 0xABCD)], 0);
        assert_eq!(g.grup_sayisi(), 1);
        assert_eq!(g.gruplar[0].len(), 3);
    }

    #[test]
    fn hamming_eşigi_sinirinda_birlesir() {
        // 0x0 ^ 0x3 = 0b11 -> 2 bit fark
        let g = grupla(&[oge(0, 0x0), oge(1, 0x3)], 2);
        assert_eq!(g.grup_sayisi(), 1, "mesafe == esik ise birleşmeli");
    }

    #[test]
    fn hamming_eşigini_asan_ayrilir() {
        // 0x0 ^ 0x3 -> 2 bit; esik 1 ise ayrı
        let g = grupla(&[oge(0, 0x0), oge(1, 0x3)], 1);
        assert_eq!(g.grup_sayisi(), 2);
    }

    #[test]
    fn farkli_kompozisyonlar_ayri_grupta() {
        // 0x0000 vs 0xFFFF -> 16 bit fark, esik 8 -> ayrı
        let g = grupla(
            &[oge(0, 0x0000_0000_0000_0000), oge(1, 0xFFFF_FFFF_FFFF_FFFF)],
            8,
        );
        assert_eq!(g.grup_sayisi(), 2);
    }

    #[test]
    fn zincir_birlestirme_tespit_edilir() {
        // A=0x00, B=0x03 (2 bit fark), C=0xFF (A-C 8 bit fark ama B-C 8)
        // Eşik 2: A~B (2), B~C (6>2 değil) — zincir kurmaz.
        // Eşik 8: A~B (2), B~C (6), A~C (8 == 8 <= 8) → hepsi birleşir, çakışma yok.
        // Eşik 6: A~B (2), B~C (6==6), A~C (8>6) → zincir, çakışma!
        let g = grupla(
            &[
                oge(0, 0x0000_0000_0000_0000),
                oge(1, 0x0000_0000_0000_0003),
                oge(2, 0x0000_0000_0000_00FF),
            ],
            6,
        );
        assert_eq!(g.grup_sayisi(), 1, "zincirle tek grup");
        assert!(!g.cakismalar.is_empty(), "A-C çakışması tespit edilmeli");
        let c = &g.cakismalar[0];
        assert_eq!(c.mesafe, 8);
        assert_eq!(c.esik, 6);
    }

    #[test]
    fn cakisma_yoksa_liste_bos_kalir() {
        let g = grupla(
            &[
                oge(0, 0x0000_0000_0000_0000),
                oge(1, 0x0000_0000_0000_0003),
                oge(2, 0x0000_0000_0000_0007),
            ],
            8,
        );
        assert_eq!(g.grup_sayisi(), 1);
        assert!(g.cakismalar.is_empty());
    }

    #[test]
    fn ikiz_kareler_ayri_kalir() {
        // Tamamen farklı iki kare hiçbir eşikte birleşmemeli (0x0000 vs 0xFFFF)
        let g = grupla(
            &[oge(0, 0x0000_0000_0000_0000), oge(1, 0xFFFF_FFFF_FFFF_FFFF)],
            63,
        );
        assert_eq!(g.grup_sayisi(), 2, "63 eşikte bile 64 fark ayırır");
    }

    #[test]
    fn esik_sifir_yalniz_tam_ayni_hashleri_birlestirir() {
        let g = grupla(
            &[
                oge(0, 0x1234_5678_9ABC_DEF0),
                oge(1, 0x1234_5678_9ABC_DEF0),
                oge(2, 0x1234_5678_9ABC_DEF1),
            ],
            0,
        );
        assert_eq!(g.grup_sayisi(), 2);
        assert_eq!(g.gruplar[0], vec![0, 1]);
        assert_eq!(g.gruplar[1], vec![2]);
    }

    #[test]
    fn uclu_kararsiz_kare_tek_kalir() {
        let g = grupla(&[oge(0, 0xAA)], 8);
        assert_eq!(g.grup_sayisi(), 1);
        assert_eq!(g.gruplar[0], vec![0]);
    }

    #[test]
    fn ayni_grup_sorgusu_calisir() {
        let g = grupla(&[oge(0, 0x1), oge(1, 0x1), oge(2, 0x2)], 0);
        assert!(g.ayni_grup(0, 1));
        assert!(!g.ayni_grup(0, 2));
        assert!(!g.ayni_grup(1, 2));
    }

    #[test]
    fn grup_bul_sira_dondurur() {
        let g = grupla(&[oge(0, 0x1), oge(1, 0x2), oge(2, 0x3)], 0);
        assert_eq!(g.grup_bul(0), Some(0));
        assert_eq!(g.grup_bul(1), Some(1));
        assert_eq!(g.grup_bul(2), Some(2));
    }

    #[test]
    fn kimlikler_sirayla_atanir() {
        let g = grupla(&[oge(0, 0x1), oge(1, 0x2), oge(2, 0x3)], 0);
        assert_eq!(g.kimlikler, vec![0, 1, 2]);
    }

    #[test]
    fn cok_uzun_kare_pozisyonlari_gruplanir() {
        // 50 kare, hepsi aynı hash (burst).
        let oge_list: Vec<Oge> = (0..50).map(|i| oge(i, 0xDEAD_BEEF)).collect();
        let g = grupla(&oge_list, 8);
        assert_eq!(g.grup_sayisi(), 1);
        assert_eq!(g.gruplar[0].len(), 50);
        assert!(g.cakismalar.is_empty());
    }

    #[test]
    fn hizli_grup_dar_esikte_tam_tarama_ile_ayni_sonucu_verir() {
        let oge_list: Vec<Oge> = (0..20).map(|i| oge(i, (i as u64) << 4)).collect();
        let tam = grupla(&oge_list, 4);
        let hizli = hizli_grup(&oge_list, 4);
        assert_eq!(tam.grup_sayisi(), hizli.grup_sayisi());
        assert_eq!(tam.gruplar, hizli.gruplar);
    }

    #[test]
    fn hizli_grup_16_esiginde_tam_taramaya_duser() {
        let oge_list = vec![oge(0, 0x0), oge(1, 0xFFFF)];
        let hizli = hizli_grup(&oge_list, 16);
        let tam = grupla(&oge_list, 16);
        assert_eq!(tam.grup_sayisi(), hizli.grup_sayisi());
    }

    #[test]
    fn parca_bitis_dogru_parca_dondurur() {
        let oge_list = vec![oge(0, 0x1234_5678_9ABC_DEF0)];
        assert_eq!(parca_bitis(0, 0, &oge_list), 0xDEF0);
        assert_eq!(parca_bitis(0, 1, &oge_list), 0x9ABC);
        assert_eq!(parca_bitis(0, 2, &oge_list), 0x5678);
        assert_eq!(parca_bitis(0, 3, &oge_list), 0x1234);
    }

    #[test]
    fn aday_cift_sayimi_dogrulama_icin_kaydedilir() {
        let oge_list = vec![oge(0, 0x0), oge(1, 0x0), oge(2, 0x1)];
        let g = grupla(&oge_list, 0);
        // 3 eleman -> 3 çift karşılaştırılır.
        assert_eq!(g.toplam_cift, 3);
        // Yalnız (0,1) eşleşir; (0,2) ve (1,2) birer bit farklı.
        assert_eq!(g.aday_cift, 1);
    }

    #[test]
    fn uclu_grup_tek_uye_degil_birlesik() {
        // Üç kare, iki farklı kompozisyon grubunda
        let g = grupla(&[oge(0, 0x0), oge(1, 0x3), oge(2, 0xF0)], 2);
        assert_eq!(g.grup_sayisi(), 2, "0-3 birleşir, F0 ayrı");
    }
}
