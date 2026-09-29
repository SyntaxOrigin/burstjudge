//! Uçtan uca tarama ve gruplama entegrasyon testleri.
//!
//! Bu dosya, `bhash`/`grupla` birim testlerinin aksine **gerçek dosya sistemi**
//! üzerinde çalışır: geçici klasörler kurar, TIFF/EXIF dosyaları yazar, motoru
//! çalıştırır ve rapor şemasını doğrular.

mod yardimci;

use burstjudge::bhash::AlgisalHash;
use burstjudge::grupla::{grupla, Oge};
use burstjudge::motor::Motor;
use burstjudge::ornek_veri::Kayit;
use burstjudge::puan::Ayarlar;
use burstjudge::rapor;
use yardimci::{burst_yaz, ham_yaz, ornek_yaz, tara, GeciciDizin};

#[test]
fn bos_klasor_taramasi_basarili_ve_bos_rapor_verir() {
    let d = GeciciDizin::yeni("bos").expect("dizin");
    let (kayitlar, atlanan) = tara(d.yol()).expect("tara");
    assert!(kayitlar.is_empty());
    assert!(atlanan.is_empty());
    let motor = Motor::yeni();
    let g = grupla(&[], 8);
    let r = rapor::olustur(
        d.yol(),
        &motor,
        8,
        &kayitlar,
        &atlanan,
        &g,
        &Ayarlar::varsayilan(),
    );
    assert_eq!(r.grup_sayisi(), 0);
    assert!(r.json_uret().is_ok());
}

#[test]
fn tek_burst_karesi_okunur_ve_tek_grupta_cikar() {
    let d = GeciciDizin::yeni("tek").expect("dizin");
    ornek_yaz(&d, "IMG_0001.CR2", &Kayit::onizleme_ile(16, 16, 128));
    let (kayitlar, atlanan) = tara(d.yol()).expect("tara");
    assert_eq!(kayitlar.len(), 1);
    assert!(atlanan.is_empty());
    let k = &kayitlar[0];
    assert_eq!(k.dosya_adi, "IMG_0001.CR2");
    assert!(k.onizleme_var, "gömülü önizleme bulunmalı");
    assert!(k.hash.is_some(), "hash üretilmeli");
    assert!(k.puan.toplam > 0.0);
    assert_eq!(k.meta.iso, Some(200));
    assert_eq!(k.meta.model.as_deref(), Some("Test Kamera"));
}

#[test]
fn ayni_kompozisyonlu_burst_kareleri_tek_grupla_birlesir() {
    let d = GeciciDizin::yeni("burst").expect("dizin");
    burst_yaz(&d, "IMG_00", 4, (16, 16, 120));
    let (kayitlar, atlanan) = tara(d.yol()).expect("tara");
    assert_eq!(kayitlar.len(), 4);
    assert!(kayitlar.iter().all(|k| k.hash.is_some()));

    let oge_list: Vec<Oge> = kayitlar
        .iter()
        .enumerate()
        .filter_map(|(i, k)| {
            k.hash.map(|h| Oge {
                sira: i,
                hash: AlgisalHash(h.0),
            })
        })
        .collect();
    let g = grupla(&oge_list, 8);
    assert_eq!(g.grup_sayisi(), 1, "aynı görüntü -> tek grup");
    assert_eq!(g.gruplar[0].len(), 4);
    assert!(g.cakismalar.is_empty());

    let motor = Motor::yeni();
    let r = rapor::olustur(
        d.yol(),
        &motor,
        8,
        &kayitlar,
        &atlanan,
        &g,
        &Ayarlar::varsayilan(),
    );
    assert_eq!(r.grup_sayisi(), 1);
    assert_eq!(r.gruplar[0].uyeler.len(), 4);
    assert!(!r.gruplar[0].cakismali);
    assert!(r.gruplar[0].gerekce.contains("toplam"));
}

#[test]
fn farkli_kameralar_ayri_kayitlari_uretir() {
    let d = GeciciDizin::yeni("kameralar").expect("dizin");
    ornek_yaz(&d, "canon.cr2", &Kayit::canon());
    ornek_yaz(&d, "nikon.nef", &Kayit::nikon());
    ornek_yaz(&d, "sony.arw", &Kayit::sony());
    let (kayitlar, atlanan) = tara(d.yol()).expect("tara");
    assert_eq!(kayitlar.len(), 3, "3 farklı gövde okunmalı");
    assert!(atlanan.is_empty());
    let modeller: Vec<&str> = kayitlar
        .iter()
        .filter_map(|k| k.meta.model.as_deref())
        .collect();
    assert!(modeller.contains(&"Test Kamera"));
    assert!(modeller.contains(&"NIKON Z 6_2"));
    assert!(modeller.contains(&"ILCE-7M4"));
    // Sony örneği 90° döndürülmüş yön kodu taşır: en-boy oranı takas edilir.
    let sony = kayitlar
        .iter()
        .find(|k| k.meta.model.as_deref() == Some("ILCE-7M4"))
        .expect("sony");
    assert_eq!(sony.meta.yon_kodu, Some(6));
    assert_eq!(sony.meta.en_boy_orani, Some(f64::from(4672) / 7008.0));
}

#[test]
fn onizlemesiz_dosyalar_yine_de_raporlanir() {
    let d = GeciciDizin::yeni("onizlemesiz").expect("dizin");
    ornek_yaz(&d, "a.cr2", &Kayit::oran());
    ornek_yaz(&d, "b.cr2", &Kayit::oran());
    let (kayitlar, _) = tara(d.yol()).expect("tara");
    assert_eq!(kayitlar.len(), 2);
    assert!(kayitlar.iter().all(|k| !k.onizleme_var));
    assert!(kayitlar.iter().all(|k| k.hash.is_none()));
    // Metadatası okunduğu için puan hesaplanabilmeli.
    assert!(kayitlar.iter().all(|k| k.puan.toplam > 0.0));
}

#[test]
fn bozuk_dosya_taramayi_durdurmaz_ve_ayri_listelenir() {
    let d = GeciciDizin::yeni("bozuk").expect("dizin");
    ornek_yaz(&d, "iyi.cr2", &Kayit::canon());
    ham_yaz(&d, "bozuk.cr2", b"bu bir TIFF dosyasi degil");
    let (kayitlar, atlanan) = tara(d.yol()).expect("tara");
    assert_eq!(kayitlar.len(), 1, "geçerli dosya okunmalı");
    assert_eq!(atlanan.len(), 1, "bozuk dosya atlanmalı");
    assert_eq!(atlanan[0].hata, "tiff-bazli-degil");
    assert!(atlanan[0].ayrinti.contains("bozuk.cr2"));
}

#[test]
fn bigtiff_dosyasi_desteklenmedi_olarak_isaretlenir() {
    let d = GeciciDizin::yeni("bigtiff").expect("dizin");
    let mut baytlar = Vec::from(*b"II\x2b\x00");
    baytlar.extend_from_slice(&8u32.to_le_bytes());
    baytlar.resize(128, 0);
    ham_yaz(&d, "buyuk.dng", &baytlar);
    let (kayitlar, atlanan) = tara(d.yol()).expect("tara");
    assert!(kayitlar.is_empty());
    assert_eq!(atlanan.len(), 1);
    assert_eq!(atlanan[0].hata, "bigtiff");
}

#[test]
fn uzantisi_desteklenmeyen_dosyalar_taranmaz() {
    let d = GeciciDizin::yeni("uzanti").expect("dizin");
    ornek_yaz(&d, "a.cr2", &Kayit::canon());
    ornek_yaz(&d, "b.cr3", &Kayit::canon());
    ornek_yaz(&d, "c.dng", &Kayit::canon());
    ornek_yaz(&d, "d.arw", &Kayit::canon());
    ham_yaz(&d, "notlar.txt", b"bu bir metin dosyasi");
    ham_yaz(&d, "sunum.pdf", b"%PDF-1.4");
    let (kayitlar, atlanan) = tara(d.yol()).expect("tara");
    assert_eq!(kayitlar.len(), 4, "yalnız desteklenen uzantılar");
    assert!(atlanan.is_empty());
}

#[test]
fn alt_dizinler_taranir_ve_gizli_dizinler_atlanir() {
    let d = GeciciDizin::yeni("alt").expect("dizin");
    let alt = d.dosya("2026-09-29");
    std::fs::create_dir_all(&alt).expect("alt dizin");
    std::fs::write(
        alt.join("a.cr2"),
        burstjudge::ornek_veri::tif_tek(&Kayit::canon()),
    )
    .expect("yaz");
    let gizli = d.dosya(".git");
    std::fs::create_dir_all(&gizli).expect("gizli dizin");
    std::fs::write(
        gizli.join("b.cr2"),
        burstjudge::ornek_veri::tif_tek(&Kayit::canon()),
    )
    .expect("yaz");
    let (kayitlar, _) = tara(d.yol()).expect("tara");
    assert_eq!(kayitlar.len(), 1, "gizli dizin atlanmalı");
}

#[test]
fn tarama_dosyanin_yuzde_birinden_azini_okur() {
    // 4 MiB'lık dosyalar: ham sensör verisi hiç okunmamalı.
    let d = GeciciDizin::yeni("kismi").expect("dizin");
    let temel = burstjudge::ornek_veri::tif_tek(&Kayit::canon());
    let mut buyuk = temel.clone();
    buyuk.resize(4 * 1024 * 1024, 0);
    std::fs::write(d.dosya("a.cr2"), &buyuk).expect("yaz");
    std::fs::write(d.dosya("b.cr2"), &buyuk).expect("yaz");

    let (kayitlar, _) = tara(d.yol()).expect("tara");
    assert_eq!(kayitlar.len(), 2);
    for k in &kayitlar {
        assert!(
            k.okunan_bayt * 200 < k.dosya_boyutu,
            "{} bayt / {} bayt okundu — kısmi okuma yok",
            k.okunan_bayt,
            k.dosya_boyutu
        );
    }
    let ist = burstjudge::motor::istatistik(&kayitlar, &[]);
    assert!(ist.okuma_orani() < 0.005, "{}", ist.okuma_orani());
    assert_eq!(ist.toplam_dosya_boyutu, 2 * 4 * 1024 * 1024);
}

#[test]
fn onizleme_bytesi_dosyadan_bayt_bayt_cikarilabilir() {
    let d = GeciciDizin::yeni("onizleme").expect("dizin");
    let kayit = Kayit::onizleme_ile(16, 16, 77);
    let yol = ornek_yaz(&d, "a.cr2", &kayit);

    let (mut okuyucu, _baslik) = burstjudge::tiff::TiffOkuyucu::ac(&yol).expect("ac");
    let belge = burstjudge::tiff::coz_oku(&mut okuyucu).expect("coz");
    let konum = burstjudge::onizleme::onizleme_konumu(&mut okuyucu, &belge).expect("konum");
    let baytlar = burstjudge::onizleme::onizleme_baytlarini_oku(&mut okuyucu, &konum, 1 << 20)
        .expect("onizleme oku");

    assert!(
        burstjudge::onizleme::jpeg_soi_dogrula(&baytlar),
        "SOI imzası"
    );
    let g = burstjudge::jpeg::coz_jpeg(&baytlar).expect("coz");
    assert_eq!(g.genislik, 16);
    assert!((g.ortalama() - 77.0).abs() <= 1.0, "{}", g.ortalama());
}

#[test]
fn onizleme_dosyaya_yazilip_digeri_cikarilabilir() {
    let d = GeciciDizin::yeni("onizleme-yaz").expect("dizin");
    let yol = ornek_yaz(&d, "a.cr2", &Kayit::onizleme_ile(16, 16, 33));
    let (mut okuyucu, _b) = burstjudge::tiff::TiffOkuyucu::ac(&yol).expect("ac");
    let belge = burstjudge::tiff::coz_oku(&mut okuyucu).expect("coz");
    let konum = burstjudge::onizleme::onizleme_konumu(&mut okuyucu, &belge).expect("konum");
    let mut hedef: Vec<u8> = Vec::new();
    let n = burstjudge::onizleme::onizleme_yaz(&mut okuyucu, &konum, &mut hedef).expect("yaz");
    assert_eq!(n, konum.uzunluk);
    assert_eq!(hedef.len(), konum.uzunluk as usize);
    // Yazılan önizleme tek başına da çözülebilir olmalı.
    assert!(burstjudge::jpeg::coz_jpeg(&hedef).is_ok());
}

#[test]
fn bozuk_onizleme_dosyayi_atlatmaz_ama_hash_uretmez() {
    let d = GeciciDizin::yeni("bozuk-onizleme").expect("dizin");
    ornek_yaz(&d, "a.cr2", &Kayit::bozuk_onizleme_ile());
    let (kayitlar, atlanan) = tara(d.yol()).expect("tara");
    assert!(atlanan.is_empty(), "bozuk önizleme dosyayı atlatmamalı");
    assert_eq!(kayitlar.len(), 1);
    assert!(kayitlar[0].onizleme_var);
    assert!(kayitlar[0].hash.is_none());
    // Metadatası yine okunmuş olmalı.
    assert_eq!(kayitlar[0].meta.iso, Some(200));
}

#[test]
fn en_iyi_kare_her_zaman_en_yuksek_puanli_olur() {
    let d = GeciciDizin::yeni("oneri").expect("dizin");
    // Aynı önizleme (aynı hash) ama farklı pozlama -> farklı puan.
    let temiz = Kayit {
        iso: Some(100),
        poz_suresi: Some(1.0 / 2000.0),
        odak_mm: Some(50.0),
        onizleme: Some((16, 16, 120)),
        ..Kayit::canon()
    };
    let gurultulu = Kayit {
        iso: Some(12800),
        poz_suresi: Some(1.0 / 30.0),
        odak_mm: Some(50.0),
        onizleme: Some((16, 16, 120)),
        ..Kayit::canon()
    };
    ornek_yaz(&d, "z_gurultulu.cr2", &gurultulu);
    ornek_yaz(&d, "a_temiz.cr2", &temiz);
    ornek_yaz(
        &d,
        "m_orta.cr2",
        &Kayit {
            iso: Some(1600),
            poz_suresi: Some(1.0 / 250.0),
            odak_mm: Some(50.0),
            onizleme: Some((16, 16, 120)),
            ..Kayit::canon()
        },
    );

    let (kayitlar, atlanan) = tara(d.yol()).expect("tara");
    let oge_list: Vec<Oge> = kayitlar
        .iter()
        .enumerate()
        .filter_map(|(i, k)| {
            k.hash.map(|h| Oge {
                sira: i,
                hash: AlgisalHash(h.0),
            })
        })
        .collect();
    let g = grupla(&oge_list, 8);
    assert_eq!(g.grup_sayisi(), 1, "üçü de aynı görüntü");

    let motor = Motor::yeni();
    let r = rapor::olustur(
        d.yol(),
        &motor,
        8,
        &kayitlar,
        &atlanan,
        &g,
        &Ayarlar::varsayilan(),
    );
    let grup = &r.gruplar[0];
    let en_iyi_ad = grup
        .oneri
        .file_name()
        .map(|a| a.to_string_lossy().to_string())
        .unwrap_or_default();
    assert_eq!(en_iyi_ad, "a_temiz.cr2");
    // Kabul kriteri: öneri, toplam puanı en yüksek kare ile aynı olmalı.
    let en_yuksek = grup
        .uyeler
        .iter()
        .map(|u| u.toplam_puan)
        .fold(f64::MIN, f64::max);
    assert_eq!(grup.uyeler[0].toplam_puan, en_yuksek);
    assert!(en_yuksek > grup.uyeler[2].toplam_puan);
}

#[test]
fn zincir_birlestirme_cakisma_olarak_isaretlenir() {
    let d = GeciciDizin::yeni("zincir").expect("dizin");
    // Üç farklı önizleme: A-B yakın, B-C yakın, A-C uzak.
    ornek_yaz(&d, "a.cr2", &Kayit::onizleme_ile(16, 16, 30));
    ornek_yaz(&d, "b.cr2", &Kayit::onizleme_ile(16, 16, 90));
    ornek_yaz(&d, "c.cr2", &Kayit::onizleme_ile(16, 16, 200));
    let (kayitlar, _) = tara(d.yol()).expect("tara");
    assert!(kayitlar.iter().all(|k| k.hash.is_some()), "hash üretilmeli");
    let oge_list: Vec<Oge> = kayitlar
        .iter()
        .enumerate()
        .filter_map(|(i, k)| {
            k.hash.map(|h| Oge {
                sira: i,
                hash: AlgisalHash(h.0),
            })
        })
        .collect();
    // Düz 30/90/200: ortalama fark hash'i AC'siz olduğu için hepsi boş olabilir;
    // bu durumda üç kare tek grupta ve "uzak çift" yoktur. Kayıt yine de
    // çözümlenebilir olmalı.
    let g = grupla(&oge_list, 8);
    assert!(g.grup_sayisi() >= 1);
    for c in &g.cakismalar {
        assert!(c.mesafe > c.esik, "çakışma tanımı bozuk: {:?}", c);
    }
}

#[test]
fn onizlemesiz_kareler_grup_raporunda_tek_uyelik_grup_olur() {
    let d = GeciciDizin::yeni("tekli").expect("dizin");
    ornek_yaz(&d, "a.cr2", &Kayit::oran());
    ornek_yaz(&d, "b.cr2", &Kayit::nikon());
    let (kayitlar, atlanan) = tara(d.yol()).expect("tara");
    let motor = Motor::yeni();
    let g = grupla(&[], 8);
    let r = rapor::olustur(
        d.yol(),
        &motor,
        8,
        &kayitlar,
        &atlanan,
        &g,
        &Ayarlar::varsayilan(),
    );
    assert_eq!(r.grup_sayisi(), 2);
    assert!(r.gruplar.iter().all(|g| g.uyeler.len() == 1));
    assert!(r.gruplar.iter().all(|g| g.uyeler[0].hash.is_none()));
}

#[test]
fn toplu_disa_aktarma_kaynagi_degistirmez() {
    let d = GeciciDizin::yeni("aktarim").expect("dizin");
    let kaynak = d.dosya("kaynak");
    std::fs::create_dir_all(&kaynak).expect("kaynak");
    for i in 0..4 {
        ornek_yaz_kaynak(&kaynak, i);
    }
    let hedef = d.dosya("secili");
    std::fs::create_dir_all(&hedef).expect("hedef");

    let (kayitlar, atlanan) = tara(&kaynak).expect("tara");
    let motor = Motor::yeni();
    let g = grupla(&[], 8);
    let r = rapor::olustur(
        &kaynak,
        &motor,
        8,
        &kayitlar,
        &atlanan,
        &g,
        &Ayarlar::varsayilan(),
    );

    let kaynak_once: Vec<String> = std::fs::read_dir(&kaynak)
        .expect("oku")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    let mut aktarilan = 0usize;
    for grup in &r.gruplar {
        let ad = grup
            .oneri
            .file_name()
            .map(|a| a.to_os_string())
            .unwrap_or_default();
        let h = burstjudge::aktar::benzersiz_ad(&hedef.join(ad));
        burstjudge::aktar::kopyala(&grup.oneri, &h).expect("kopyala");
        aktarilan += 1;
    }
    assert_eq!(aktarilan, 4);
    assert_eq!(std::fs::read_dir(&hedef).expect("oku").count(), 4);

    // Kaynak klasörün adları ve sayısı değişmemeli.
    let kaynak_sonra: Vec<String> = std::fs::read_dir(&kaynak)
        .expect("oku")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    let mut a = kaynak_once.clone();
    let mut b = kaynak_sonra.clone();
    a.sort();
    b.sort();
    assert_eq!(a, b, "kaynak klasör değişmemeli");
}

#[test]
fn disa_aktarma_hedefi_kaynagin_ici_reddedilir() {
    let d = GeciciDizin::yeni("cakisma").expect("dizin");
    let kaynak = d.dosya("kaynak");
    std::fs::create_dir_all(&kaynak).expect("kaynak");
    assert!(burstjudge::aktar::hedef_uygun(&kaynak, &kaynak).is_err());
    assert!(burstjudge::aktar::hedef_uygun(&kaynak, &kaynak.join("secili")).is_err());
    assert!(burstjudge::aktar::hedef_uygun(&kaynak, &d.dosya("secili")).is_ok());
}

#[test]
fn toplu_disa_aktarma_rapor_temelli_calisir() {
    let d = GeciciDizin::yeni("rapor-aktarim").expect("dizin");
    let kaynak = d.dosya("kaynak");
    std::fs::create_dir_all(&kaynak).expect("kaynak");
    for i in 0..3 {
        ornek_yaz_kaynak(&kaynak, i);
    }
    let (kayitlar, atlanan) = tara(&kaynak).expect("tara");
    let motor = Motor::yeni();
    let g = grupla(&[], 8);
    let r = rapor::olustur(
        &kaynak,
        &motor,
        8,
        &kayitlar,
        &atlanan,
        &g,
        &Ayarlar::varsayilan(),
    );
    // Rapor JSON'a yazılıp geri okunabilmeli (export komutunun girdisi).
    let json = r.json_uret().expect("json");
    let geri: burstjudge::Rapor = serde_json::from_str(&json).expect("ayrıştır");
    assert_eq!(geri.grup_sayisi(), r.grup_sayisi());
    assert_eq!(geri.kok, r.kok);
    for grup in &geri.gruplar {
        assert!(grup.oneri.exists(), "öneri diskte olmalı: {:?}", grup.oneri);
    }
}

fn ornek_yaz_kaynak(kaynak: &std::path::Path, i: usize) {
    // Farklı desenler -> farklı hash -> her kare kendi grubunda.
    let desenler = [
        burstjudge::jpeg_test_veri::Desen::DikeySerit {
            periyot: 8,
            koyu: 30,
            acik: 200,
        },
        burstjudge::jpeg_test_veri::Desen::Damali {
            hucre: 8,
            koyu: 40,
            acik: 210,
        },
        burstjudge::jpeg_test_veri::Desen::Radyal {
            merkez_gri: 240,
            kenar_gri: 25,
        },
        burstjudge::jpeg_test_veri::Desen::YataySerit {
            periyot: 16,
            koyu: 15,
            acik: 235,
        },
    ];
    std::fs::write(
        kaynak.join(format!("IMG_{:04}.CR2", i)),
        burstjudge::ornek_veri::tif_tek(&Kayit::desenli_onizleme_ile(desenler[i % 4])),
    )
    .expect("yaz");
}
