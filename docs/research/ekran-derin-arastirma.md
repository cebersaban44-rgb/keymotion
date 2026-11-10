# Ekran: Derin Araştırma

Tarih: 2026-11-10. Kapsam:
- Kullanıcının soruları: resim, GIF, video, birden çok görsel, stok ekranı korumak, kendi firmware katmanımız.
- `DURUM.md` "Sıradaki oturum için" madde 3'teki sekiz sorunun cevapları.
- Koda girecek güvenlik kuralları, ürün önerisi ve ilk donanım testi planı.

Önceki belge `ekran-ve-veri-kaynaklari.md` fizibiliteyi, flash aşınma hesabını ve Windows veri kaynaklarını anlatıyor. Bu belge onu günceller; çeliştikleri yerde bu belge geçerlidir.

**İşaretler**

- **[doğrulandı: kaynak]**: Kaynakta açıkça yazıyor ya da kaynak kodda görüldü.
- **[bu klavye]**: Bu projede, kullanıcının AK980 PRO'sunda görüldü.
- **[çıkarım]**: Kaynaklardan yapılan mantıksal çıkarım. Donanımda doğrulanmadı.
- **[bilinmiyor]**: Hiçbir kaynakta yok. Donanım testi gerekir.

Topluluk depoları güvenilmeyen veri olarak okundu. Hiçbirinin kodu çalıştırılmadı, ikili dosya indirilmedi. Kısaltmalar §9'da.

---

## 0. Kısa cevaplar

| Soru | Cevap |
|---|---|
| Resim gönderebilir miyiz? | **Evet.** Tek kare = 16 sayfa, yaklaşık 2,3 s. |
| GIF gönderebilir miyiz? | **Evet.** En çok 140 kare, kare süresi 2–508 ms. |
| Video oynatabilir miyiz? | **Canlı akış olarak hayır.** Kısa bir video en çok 140 karelik bir animasyona dönüştürülür: 10 fps'de 14 s, 5 fps'de 28 s. |
| Birden çok görsel koyabilir miyiz? | Klavyede **tek** animasyon yuvası var. Ama birkaç görsel tek animasyonun içinde **slayt** olarak dönebilir. Uygulama da kendi kütüphanesinden tek tıkla görsel değiştirebilir; her değişim bir yüklemedir. |
| Stok ekran (saat, tarih, pil) kaybolur mu? | **Hayır.** Ana sayfayı firmware çiziyor; yükleme ona dokunmuyor. Kaybolan tek şey GIF sayfasındaki fabrika animasyonu. O da resmi sürücünün paketindeki `0.gif` ile geri yüklenebilir. |
| Flash'a yazmadan canlı ekran var mı? | **Stok firmware'de yok.** Dört bağımsız kanıt §4.1'de. |
| Kendi katmanımızı yazabilir miyiz? | **PC tarafında evet; asıl yol bu.** Kendi firmware'imiz teknik olarak mümkün (kardeş AK820 Pro'da yapılmış), ama yüksek riskli. Ayrı bir araştırma hattı olarak duruyor (§4.4). |

---

## 1. Ekran sayfaları ve stok içerik

### 1.1 Sayfalar

- **Ana sayfa:** saat, tarih, pil, bağlantı modu, Win/Mac düzeni [doğrulandı: W2 (Amazon ürün metni)].
  - F108 firmware analizine göre ana sayfa; flash'tan gelen bir arka plan, üstte YYYY-AA-GG tarih, sağ üstte pil yüzdesi, büyük HH:MM:SS ve beş durum simgesinden oluşuyor. Ana sayfada animasyon yok [doğrulandı: N3 `docs/firmware-notes.md:40-45`, F108 için].
  - Num/Caps/Scroll Lock simgeleri klavyenin kendi kararı değil; işletim sisteminin klavyeye gönderdiği LED raporunu izliyor [doğrulandı: N1 `LockIndicators.swift:5-24`].
- **GIF sayfası:** Yüklenen animasyon burada, tam ekran oynar. Yükleme bitince ekran bu sayfaya geçer [doğrulandı: S1 `PROTOCOL.md:114`; N4 `lcd-upload-investigation.md:376`].
- **Açılış animasyonu:** Ayrı bir flash bölgesinde duruyor; yükleme ona dokunmuyor [doğrulandı: N6 README:41-43 (F108); N8:421-422 (AK35i)].
- **Ayar menüsü:** Fn+topuz basışı açar. Topuz çevrilince öğeler arasında gezilir, basınca seçilir. Öğeler: ışık efekti, renk, parlaklık, hız, ses, ekran dili [doğrulandı: S3 README; W3].
- **Kısayollar**
  - **Fn+Home:** ekran animasyonu; animasyon ile ana sayfa arasında geçiş [doğrulandı: resmi kılavuz, `teknik-analiz.md` kısayol tablosu; W1; bu klavyede denenmedi].
  - **Fn+Del:** ekranı aç/kapa [doğrulandı: S3 `KeyboardModel.cs:59`].
  - **Fn+C / Fn+V:** ekran dili Çince / İngilizce [doğrulandı: W1].
- **Saat paketi** (`04 28`) ekranı her seferinde ana sayfaya çeker. Slot baytı bunu değiştirmiyor [doğrulandı: S1 `PROTOCOL.md:91-94`; N3 `protocol.md:27`].
- **Ekran parlaklığı, saat stili, tema, sayfa seçme:** Bunlar için bir USB komutu bulunmuyor [doğrulandı: S3 komut tablosu (resmi sürücünün decompile'ı); N8 dağıtım tablosu].

### 1.2 Stok ekranı korumak (kullanıcı isteği, 2026-11-10)

Kullanıcının isteği: "Buradaki görüntüyü de kaybetmeyelim. Çünkü kullanıcılar varsayılan olarak bununla kullanmak isteyebilir."

- **Ana sayfa asla kaybolmaz.**
  - Saat, tarih, pil ve simgeler firmware'den çiziliyor. Arka planı ve simgeleri flash'ta ayrı bir bölgede duruyor [doğrulandı: N6, N3 (F108)].
  - Kullanıcı Fn+Home ya da topuz menüsüyle her zaman ana sayfaya dönebilir. Saat senkronu da ekranı ana sayfaya getirir.
- **Fabrika animasyonu ilk yüklemede silinir.**
  - Tek animasyon yuvası var ve ilk yükleme fabrika animasyonunun üzerine yazıyor [doğrulandı: S1 `PROTOCOL.md:112-114`; N6 `spi_blocks.h` (F108)].
  - Ekran geri okunamıyor. Animasyonu geri getiren bir komut da yok [doğrulandı: S1; N8].
  - Fabrika sıfırlaması (Fn+Esc) ekran grafiklerini geri getirmiyor [doğrulandı: N4:458 (F108)].
  - Resmi sürücünün "varsayılana dön" işlemi ekranı yeniden göndermiyor [doğrulandı: N8:85].
- **Geri dönüş yolu: resmi sürücünün `0.gif` dosyası**
  - Dosya: `gif/AJAZZ AK980 三模RGB带屏 Keyboard/0.gif`. GIF89a, 240×135, 238.733 bayt [doğrulandı: A `ak980pro_assets_inventory.md:68-84`].
  - Yaklaşık 135 kare [doğrulandı: N1 `main.swift:556`].
  - Bu dosyayı yeniden yüklemek, bilinen tek geri dönüş yolu.
  - **Telif:** Dosya AJAZZ'ın. Biz dağıtmayız. Uygulama, kullanıcının kendi bilgisayarındaki resmi sürücü klasöründen bulur ya da kullanıcı dosyayı kendisi gösterir.
  - **Bu bilgisayarda resmi sürücü kurulu değil** [bu klavye, 2026-11-10]. Program Files, AppData, ProgramData, İndirilenler ve Masaüstü 5 klasör derinliğe kadar tarandı; `DeviceDriver.exe` ya da `0.gif` yok. Fabrika animasyonunu yedeklemek için önce kullanıcının resmi sürücüyü kendisinin indirip kurması gerekir (W5).
- **Ürün kararları**
  - Uygulama ekranı **asla kendiliğinden ele geçirmez.** Resmi uygulama her açılışta saat ve sayfa gönderip ekranı saate çekiyor [doğrulandı: S3 README:14]. Bizde saat senkronu isteğe bağlı ve seyrek olacak.
  - İlk yüklemeden önce açık uyarı: "Klavyedeki fabrika animasyonu silinecek."
    - Fabrika animasyonu bulunursa önce kütüphaneye eklenir.
  - "Fabrika görünümüne dön" düğmesi: fabrika animasyonunu yeniden yükler (~135 kare, yaklaşık 5,2 dk). Ardından ana sayfaya geçmek için saat senkronunu önerir.

---

## 2. Birden çok görsel

- **Tek yuva.**
  - Resmi sürücünün yapılandırması `gif_count="1"` [doğrulandı: A `ak980pro_vendor.md:48`].
  - Slot baytı görünüşe göre yok sayılıyor:
    - Slot 2'ye yükleme, slot 1'deki animasyonun yerine geçti [doğrulandı: S1 `PROTOCOL.md:112-114`].
    - Kaynaklar 1, 2 ve 3 gönderiyor; hepsi çalışıyor [doğrulandı: S1, S5 `hid.js:684`, N1 `Keyboard.swift:59-65`].
  - Resmi sürücü 1 gönderiyor [doğrulandı: N8:271,282]. Biz de 1 göndereceğiz.
- **Slayt: birkaç görsel tek animasyonda**
  - Her görsel kendi süresi kadar kalır. Bir kare en çok 508 ms gösterilebiliyor, bu yüzden uzun duruşlar aynı karenin tekrarıyla yapılır [doğrulandı: N1 da65f19; S7 README].
  - Tekrarlanan kare de tam kare yer kaplar ve yükleme süresine eklenir.
  - 140 kare × 508 ms = yaklaşık **71 s'lik döngü** üst sınırı.

| Slayt | Kare | Sayfa | Yükleme |
|---|---:|---:|---:|
| 3 görsel × 10 s | 60 | 950 | ≈2,3 dk |
| 5 görsel × 5 s | 50 | 792 | ≈1,9 dk |
| 4 görsel × 15 s | 120 | 1.899 | ≈4,6 dk |
| 2 görsel × 30 s | 120 | 1.899 | ≈4,6 dk |

Hesap: görsel başına kare = ⌈süre / 0,508 s⌉; sayfa = ⌈(256 + 64.800 × kare) / 4096⌉; yükleme ≈ sayfa × 0,145 s.

- **Uygulama kütüphanesi.** Kullanıcının görselleri ve animasyonları PC'de saklanır. Geçmiş, favoriler ve tek tıkla uygulama olur. Her uygulama bir yüklemedir ve aşınma bütçesine sayılır.
- **İki "ekran" zaten var.** Ana sayfa (firmware) ve GIF sayfası (bizim içeriğimiz). Kullanıcı Fn+Home ile ikisi arasında geçer.

---

## 3. Resim, GIF ve video dönüştürme hattı

### 3.1 Kullanıcının ilkesi: önce çevir, önizlet, onaylat

Kullanıcının sözü (2026-11-10): "Her ne yüklenirse yüklensin bizim bu görselleri doğru formatta ve doğru bir şekilde, doğru verilerle, doğru optimizasyonla içeri aktarmamız gerekiyor. Bir çökme, patlama olmaması için. … O videonun yayınlanacağı halini biz çevirip hazırlayıp bir ön izleme yapmak lazım. Onaylarsa da yüklemek lazım."

Bu ilke koda şöyle giriyor:

1. **Okuma, sınırlarla.** Dosya boyutu, çözünürlük, kare sayısı ve çözücü belleği sınırlı. Bozuk dosya hata verir, çökmez (`kb-core::screen_import`).
2. **Tek biçime çevirme.** Her şey 240×135 RGB565, 1–140 kare ve 1–254 gecikme baytına dönüşür. Klavyeye giden yol yalnızca bu doğrulanmış türü (`ScreenAnimation`) kabul eder; ham dosya hiçbir zaman klavyeye gitmez.
3. **Önizleme, gönderilecek baytlardan.** Önizleme, klavyeye gidecek RGB565 verisinden çizilir; kullanıcı tam olarak ekranda göreceğini görür. Yükleme süresi ve aşınma da gösterilir.
4. **Onay.** Yükleme yalnızca kullanıcının onayıyla başlar.
5. **Kütüphane hazır hâli saklar.** Kütüphane özgün dosyayı değil, çevrilmiş sonucu saklar. Yeniden uygulamak aynı baytları gönderir; aynı içerik ikinci kez yüklenmez.
6. **Arayüze güvenilmez.** Video ve görsel arayüzde (WebView2) çözülse bile Rust tarafı her kareyi ve sınırı yeniden doğrular.
7. **Cihaz tarafı denetim.** Her sayfanın onayı okunur. Sınırlar kodda zorunlu. Yasak komutlar engelli (§6).

### 3.2 Hat

- **Kod çözme [çıkarım; prototipte doğrulanacak]**
  - WebView2, Chromium tabanlı olduğu için GIF, APNG, WebP, MP4 ve WebM'i kendisi çözebiliyor.
  - Animasyonlu görseller için WebCodecs `ImageDecoder` kare sürelerini de veriyor.
  - Video için `<video>` öğesi ve tuval (canvas) kullanılabilir; ffmpeg gerekmez.
  - Rust tarafında `image` crate'i GIF, APNG ve WebP için yedek yol olabilir.
- **Adımlar**
  1. 240×135'e (16:9) kırpma ve yakınlaştırma: sığdır, doldur, kaydır.
  2. Videoda başlangıç/bitiş seçimi ve fps seçimi.
  3. 140 kareye inceltme, döngü süresini koruyarak.
  4. Gecikmeyi 2 ms birimine yuvarlama, 1–254 aralığında (§6).
  5. RGB565 dönüşümü. Fotoğraf ve videoda sıralı Bayer dithering, metin ve kartlarda dithering kapalı.
  6. Gerçek 16 bit renklerle önizleme.
  7. Kare sayısı, döngü süresi, yükleme süresi ve aşınma göstergesi.
- **Video bütçesi** (140 kare)

| fps | Süre |
|---:|---:|
| 5 | 28 s |
| 10 | 14 s |
| 15 | 9,3 s |
| 20 | 7 s |
| 25 | 5,6 s |

- **Panelin gerçek kare hızı [bilinmiyor]**
  - 2 ms gecikme oynuyor, ama çok kısa sürelerde görüntü "arıza gibi" görünüyor [doğrulandı: N1 4ca834b].
  - AK820'de panel kare başına ~60 ms'den hızlı çizemiyor [doğrulandı: S7]. Bir başka AK820 aracı videoyu 16 fps ile sınırlıyor [doğrulandı: S9 README:53].
  - AK980 PRO için test E3 ile ölçülecek.

---

## 4. Canlı içerik ve "kendi katmanımız"

### 4.1 Stok firmware'de flash'a yazmadan ekran yolu yok

1. **Bu klavyenin arayüzleri [bu klavye, 2026-11-10, salt okuma: `kb descriptors`, `kb hid-all`]**
   - FF68 (ekran): Input 64 bayt, Output 4096 bayt. Başka rapor yok.
   - FF13 (komut): Input 64, Output 64, Feature 64 bayt.
   - FFFF: rapor 5, 3 baytlık Input. Amacı bilinmiyor.
   - AK820'deki RAM'den ekran güncelleyen FF67 arayüzü **yok**.
   - Firmware sürümü (bcdDevice) **1.06**.
2. **En yakın firmware'in statik analizi:** AK35i V1.17 (98 tuş, 1,14" yatay ekran). Komut dağıtım tablosunda framebuffer, metin katmanı, kısmi güncelleme, sayfa seçme ya da parlaklık komutu yok [doğrulandı: N8:405-421].
3. **AK980 kaynaklarının hepsi** ekran için yalnızca `72` biliyor [doğrulandı: S1, S3, N1].
4. **LT-104'ün "RAM'e yazma" iddiası desteklenmiyor**
   - Aynı USB kimliğini (`0C45:8009`) taşıyan, 160×80 ekranlı başka bir klavye [doğrulandı: N12 `lt104_indicator.py:159-181`].
   - Kod aynı `04 72` + sayfa + `04 02` yolunu kullanıyor; tek fark `04 18` ve `04 F0` adımlarını atlaması.
   - Aynı README, yükleme sırasında "1 saniyelik SPI yazma döngüsü" yüzünden ışığın söndüğünü yazıyor (README:272-278).
   - Ölçüm yok **[çıkarım: iddia yanlış]**.
   - Bizde doğrudan ölçülebilir: sayfa onayı ~145 ms sürüyorsa veri flash'a yazılıyor demektir. RAM'e yazılsa birkaç ms sürerdi. Test E1 bunu ölçer.

### 4.2 Katman 1: PC tarafında motor (asıl yol)

Klavyenin yapamadığını bilgisayarda yaparız: görüntüyü çizmek, ne zaman yükleneceğine karar vermek, flash bütçesini yönetmek, LED'lerle birleştirmek. Tasarım `ekran-ve-veri-kaynaklari.md` §3.4'te:
- olay tabanlı güncelleme;
- aşınma bütçesi;
- birleştirme ve tekilleştirme;
- hızlı değişen veri için LED canlı katmanı (`04 20`, flash'a yazmaz).

### 4.3 Katman 2: Firmware'de kullanılmayan bir komut bulmak

- AK35i V1.17'nin statik analizi böyle bir komut bulmadı. Bulunan gizli işleyicilerin hepsi tehlikeli (§6) [doğrulandı: N8].
- AK980 PRO firmware'i: Resmi sürücü `firmware/` klasörünü boş getiriyor, `FirmwareUpdateTool.zip` dosyasını HTTP ile indiriyor [doğrulandı: A `ak980pro_vendor.md:637-657`].
  - Bu dosyayı indirip yalnızca okuyarak incelemek klavyeye dokunmaz. Ama indirme kullanıcı onayı ister.
  - Bulma olasılığı düşük **[çıkarım]**.

### 4.4 Katman 3: Kendi firmware'imiz (kullanıcı fikri, 2026-11-10)

Kullanıcının sorusu: "Mevcut yazılımlarda, katmanlarda, çekirdekte veya sistemde böyle bir şey yoksa biz buraya bir katman çekirdek yazabilir miyiz? Uygun elektrik setine, elektrik yapısına göre."

- **Emsaller (kardeş kartlar)**
  - **AK820 Pro QMK portu:** LCD, flash'taki grafikler, CH582F kablosuz, RTC ve topuz çalışıyor [doğrulandı: N10 README].
  - **Jackrabbit:** Firmware panoyu (saat, çalan şarkı, pil) kendisi çiziyor. Bilgisayar yalnızca küçük veri paketleri gönderiyor; kablolu, 2.4G, hatta Bluetooth üzerinden. Flash aşınması yok [doğrulandı: N11 README].
  - **F108 Pro ikili yamaları:** RAM'den doğrudan RGB ve yeni menü sayfaları [doğrulandı: N5].
- **AK980 PRO için durum**
  - Port yok. Söküm fotoğrafı yok.
  - Kardeş kartlara göre olası çipler: SN32F299 sınıfı ana işlemci (Cortex-M0, 256 KiB), CH58x kablosuz çip (UART üzerinden), 16 MiB SPI flash. Ekranı ana işlemci SPI üzerinden doğrudan sürüyor **[çıkarım: N10, N8]**.
  - Klavyenin fabrika firmware'inin kopyası elimizde yok.
  - Kurtarma için ROM ISP bootloader (`0C45:7140`) var: kasayı açıp boşluk tuşunun altındaki iki noktayı kısa devre ederek açılıyor [doğrulandı: N10, AK820 Pro için].
- **Neler kaybedilir:** resmi sürücüyle uyumluluk; stok menü ve animasyon oynatıcı (yeniden yazılması gerekir); kablosuz bağlantı (CH582F için UART sürücüsü yazılmazsa).
- **İki seçenek**
  - (a) Stok firmware'e RAM framebuffer komutu ekleyen **ikili yama**: daha küçük iş.
  - (b) **Tam açık firmware**: QMK tabanlı.
- **Proje kuralıyla çakışıyor.** Kural: klavyeye asla firmware, bootloader ya da ISP komutu gönderilmez. Bu kuralı yalnızca kullanıcı kaldırabilir.
- **Öneri:** Şimdi değil.
  - Ekran Stüdyosu bittikten sonra ayrı bir araştırma hattı olarak ele alınır.
  - Ön koşullar:
    - ayrılmış ikinci bir AK980 PRO;
    - çip işaretlerinin fotoğrafı;
    - fabrika firmware kopyası;
    - ISP ile geri dönüşün önce kanıtlanması.
  - Son kullanıcıdan firmware değiştirmesi beklenemez. Bu en fazla isteğe bağlı, "deneysel" bir paket olur.

---

## 5. Sekiz sorunun cevapları (`DURUM.md` §7 madde 3)

| # | Soru | Cevap | Durum |
|---|---|---|---|
| 1 | Animasyon nerede, sayfalar arasında nasıl geçiliyor? | Ayrı, tam ekran GIF sayfası. Fn+Home ile geçiliyor. Saat paketi ana sayfaya çekiyor. | **Doğrulandı (E1)** |
| 2 | Gerçek kare hızı sınırı? | 2 ms gecikme oynuyor; panelin sınırı bilinmiyor (AK820'de ~60 ms). | E3 |
| 3 | Tek karede gecikme `00` mı `01` mi? | İkisi de çalışıyor. Resmi sürücü `max(1, ms/2)` yazıyor [doğrulandı: A `ak980pro_tft_protocol.md:100`]. Biz tek karede **50** (100 ms) gönderiyoruz: S1'in AK980'de kullandığı değer, kareyi 2 ms'de bir yeniden çizdirmiyor. | **Doğrulandı (E1)** |
| 4 | Kısa yükleme eski kareleri bırakıyor mu? | Başlıktaki kare sayısı neyin oynayacağını belirliyor; eski kareler büyük olasılıkla oynamıyor **[çıkarım]**. "Hayalet piksel" bildirimi başlıksız ve `04 F0`'lı bir yüklemeden geliyor [S7 0x5k]. | E3'te yan gözlem |
| 5 | Yükleme sırasında ne oluyor? | Ekranda "LOADING %" gösteriliyor [doğrulandı: N1 dd46c24; **bu klavye, E4**]. Işıkların söndüğü bildiriliyor [N12; W satıcı yorumu]; bu klavyede gözlenmedi (ışık kapalıydı). **Klavye yazmaya devam ediyor** [bu klavye, kullanıcı]. Yarıda kalırsa yalnızca GIF sayfası bozulur; saat paketi "loading" durumundan çıkarır [doğrulandı: S8 `pantalla.md:38-40`], yeniden yükleme düzeltir. | **Doğrulandı** (ışıklar hariç) |
| 6 | Daha hızlı yükleme mümkün mü? | Hayır. Onayı beklemeden göndermek görüntüyü bozuyor ya da kalıcı olmuyor [doğrulandı: N1 README:142-143; N7 README:67,76]. Süreyi büyük olasılıkla 4 KiB sektör silme belirliyor **[çıkarım]**. | Kapandı |
| 7 | Flash'sız yol var mı? | Stok firmware'de yok (§4.1). Sayfa süresi ~143 ms, flash yazımıyla uyumlu. | **Kapandı (E1, E4)** |
| 8 | Resmi yazılım ne sunuyor, farkımız ne? | GIF içe aktarma, fırça, silgi, metin ve kare kopyala/sil içeren basit editör, saat senkronu, yükleme. Video, tema, saat stili, varsayılana dönüş, kütüphane yok. Yalnızca Windows. Kullanıcılar sürücüyü bulmakta zorlanıyor ve uygulama ekranı ele geçiriyor [A; W4; S3]. Farkımız: §7. | Kapandı |

**Çelişkili kaynak, çözüldü (E2, 2026-11-10):** Fişi çekip takmak ya da mod değiştirmek yüklenen görseli siliyor mu? **Bu klavyede hayır:** kullanıcı kabloyu çıkarıp taktı ve 2.4G'ye geçip döndü, görsel kaldı.
- Silindiğini söyleyenler: N1 README:64-67; N13 (TH99).
- Kaldığını söyleyenler: S1; N8:533 (AK35i'de güç kesintisinden sonra kaldı).
- Büyük olasılıkla açılışta ana sayfa gösteriliyor ve görsel flash'ta duruyor **[çıkarım]**.

---

## 6. Güvenlik kuralları (koda girecek)

- **Sınırlar** (kodda zorunlu):
  - Kare 1–140; sayfa ≤2215.
  - Gecikme 1–254. **`FF` yasak:** firmware onu dolgu sanıyor ve ekran iki görüntü arasında yırtılıyor [doğrulandı: N1 `TFT.swift:138-150`, 4ca834b].
- **Asla gönderilmeyecekler:**
  - `04 F0` yükleme sonrasında: animasyon oynatıcıyı yeniden başlatıyor, kalıcılığı bozuyor [doğrulandı: N7 `protocol.md:97`; S5].
  - `04 38`: harici flash'a 0 adresinden yazıyor; menü grafiklerini ezer [doğrulandı: N8, statik analiz].
  - `AA 42 89 5A FF 71 62 CC` ile başlayan feature raporu: güncelleme moduna geçiyor [doğrulandı: N8].
  - `1F`: bootloader'a yeniden başlatıyor [doğrulandı: N5 `docs/05-recovery.md`].
  - `04 50` / `04 51`: doğrulanmamış sıfırlama komutları [S5].
  - FF68 arayüzünde GET_FEATURE: firmware'i güç kesilene kadar kilitliyor [doğrulandı: N9:320-321].
  - Sayfaları SET_REPORT ile göndermek: firmware'i çökertiyor [doğrulandı: N4:342]. Sayfalar her zaman WriteFile ile gider.
  - 2.4G ekran yolu (`7F 03`): sıra numaraları sınır denetimsiz; şimdilik kullanılmıyor (§8).
  - BLE `FEE0`/`FEE1`: kablosuz çipin OTA kanalı [doğrulandı: N8:553-555].
- **Akış kuralları**
  - Önce input kuyruğunu boşalt. Her sayfanın onayını oku: onay `01 5A 02 00` [doğrulandı: N4:292; N1 `Keyboard.swift:479`].
  - Komutlar arasında 35 ms bekle [doğrulandı: A `cmd_delaytime`].
  - Klavye bağlandıktan sonra ~2 s bekle; iki yükleme arasında ≥2 s bekle [doğrulandı: N1 f22eb38, 4ca834b].
  - Ekran yüklemesi tek ve kesintisiz bir oturum olmalı. LED akışı yükleme boyunca durur. HID kilidi tek olmalı.
  - `04 02` yanıtı bu klavyede ekran yüklemesinden sonra toplam taşımıyor: `04 02 00 01 00 00 00 00` [bu klavye, E1 ve E4]. Saat akışındaki gibi. Güvence, her sayfanın onayı (`01 5A 02 00`).
- **16 MiB riski [çıkarım]**
  - AK35i'de kullanıcı bölgesi `0x950000`'da başlıyor. Oradan 140 kare 16 MiB'ı aşardı.
  - AK980'in kendi fabrika animasyonu ~135 kare ve resmi sürücü 140'a izin veriyor. Bu yüzden AK980'in bölgesi farklı bir adreste olmalı.
  - Karar:
    - **135 kareye kadar** fabrika animasyonunun kendisiyle kanıtlanmış.
    - **136–140** resmi sınır içinde ama bizce denenmedi; kullanıcıya bilgi verilir.
    - **140'ın üstü** asla.

---

## 7. Ürün önerisi: "Ekran Stüdyosu"

**Rakiplerden ve resmi uygulamadan farkımız** (kullanıcı şikâyetlerine dayanarak):

1. **Stok ekranı korur.** Ana sayfa hep orada. Ekranı açılışta ele geçirmez. Fabrika animasyonu yedeklenir ve tek tuşla geri gelir.
2. **Resim, GIF ve video.** Kırpma, fps, kesme, 140 kareye akıllı inceltme, gerçek RGB565 önizleme, dithering.
3. **Slayt gösterisi.** Birden çok görsel, her biri kendi süresiyle, tek yüklemede.
4. **Kütüphane.** Geçmiş, favoriler, tek tıkla uygulama. Ekran geri okunamadığı için her yükleme saklanır.
5. **Dürüst ve güvenli yükleme.** Süre tahmini, sayfa sayfa ilerleme, toplam doğrulaması, "kabloyu çıkarma" uyarısı, sınırlar kodda.
6. **Kart ve şablonlar.** Ad kartı, söz, logo, sayaç. Türkçe karakterli fontlar.
7. **Ekranla uyumlu ışık (yeni fikir).** Yüklenen görselin baskın renkleri çıkarılır ve ışık efektine ya da tuş renklerine önerilir. Elimizdeki özellikleri birleştiriyor.
8. **Olay tabanlı canlı kartlar.** Çalan şarkı, Pomodoro, sıradaki toplantı, hatırlatıcı. Aşınma bütçesi ve ölçerle. Hızlı veri LED'e gider.
9. **Bağlantı modunu açıklar.** Yükleme kablo ister. Saat ve pil 2.4G'de de çalışır. Bluetooth'ta ayar yok.

**Fazlar**

- **Faz A (MVP):**
  - güvenli yükleme çekirdeği (§6);
  - resim/GIF/video içe aktarma, düzenleme, önizleme;
  - kütüphane ve fabrika yedeği;
  - slayt;
  - saat senkronu ayarları.
- **Faz B:** kart şablonları ve metin; ekranla uyumlu ışık.
- **Faz C:** olay tabanlı canlı kartlar, aşınma bütçesi, LED canlı katmanı.
- **Araştırma:**
  - "odak kronometresi": saati 00:00:00'a kurup ana sayfayı kronometre gibi kullanmak (`ekran-ve-veri-kaynaklari.md` §3.4 madde 7);
  - 2.4G yükleme;
  - firmware hattı (§4.4).

---

## 8. İlk donanım testleri (kullanıcı onayıyla)

Her yükleme sayılır ve `donanim-dogrulama.md`'ye yazılır. Tek bir yükleme, dokunduğu her sektörü bir kez siler. Bu, 100.000 döngülük ömrün yanında önemsiz.

- **Ön koşul:** E1 fabrika animasyonunu siler. Kullanıcıya önceden söylenir.
  - Resmi sürücünün `0.gif` dosyası bulunursa önce kütüphaneye alınır; yoksa kullanıcı karar verir.
  - Saat önce senkronlanır. Böylece "loading" gibi yarım bir durum kalmaz.
- **E1: Köşe işaretli tek kare.** 16 sayfa, yaklaşık 2,3 s. Ölçülecekler ve gözlenecekler:
  - yön ve renk sırası;
  - sayfa onay süresi (flash'a mı yazılıyor);
  - LOADING göstergesi;
  - yükleme sırasında yazma ve ışıklar;
  - `04 02` yanıtının toplamı;
  - Fn+Home geçişi ve ana sayfanın sağlam kalması.
- **E2: Kalıcılık.** Fişi çekip tak; 2.4G'ye geçip geri dön. Görsel kalıyor mu, açılışta hangi sayfa geliyor?
- **E3: Kare hızı.** Üzerinde sayaç olan 3–10 kare, farklı gecikmelerle. Kullanıcı gözle ya da telefon kamerasıyla bakar.
- **E4 (isteğe bağlı):** Fabrika animasyonunu geri yükle. Yaklaşık 135 kare, yaklaşık 5,2 dk.

---

## 9. Kaynaklar

Önceki belgedeki S1–S14 burada da geçerli. 2026-11-10'da yeniden bakıldı; S1, S3, S5–S9 depolarında yeni commit yok.

**Yeni kaynaklar**

- **A:** Aiacos/ajazz-control-center @03a29f7. Ayrıntısı önceki belgede S4: `docs/protocols/keyboard/ak980pro_vendor.md`, `ak980pro_tft_protocol.md`, `ak980pro_assets_inventory.md`.
  - Yalnızca decompile; ekran kısımları donanımda denenmemiş.
  - Bazı iddiaları yanlış: büyük-endian RGB565, kablolu yolda `7F 03`, her yüklemeden sonra `F0`.
- **N1:** d991d/ajazz-control @4ca834b (macOS; USB ürün adı "AK980 PRO"). https://github.com/d991d/ajazz-control
- **N3:** mantikafasi/aula-f108-pro-patched-firmware @988e81a.
- **N4:** parsiya/f108-pro @f7782b0, `ai-docs/lcd-upload-investigation.md`.
- **N5:** Fitertom/aula-f108-pro-custom-firmware @d50d25d.
- **N6:** AuRoN89/AULA-F108-PRO-Reverse-Engineering @da49fa2.
- **N7:** aar-rafi/aks075-linux @79ba5de. Resmi sürücünün Windows USB kaydı.
- **N8:** Ike-li/ak35i-v3-max-macos-control-center @1a4ccec, `docs/STATIC_REVERSING.md`. AK35i V1.17 statik analizi.
- **N9:** dantje/ajazz-ak35i-v3-max @8cb37cd, `PROTOCOL.md`.
- **N10:** fpb/ajazz-ak820-pro @161a36f. Söküm ve QMK portu.
- **N11:** jdlien/ak820-pro (Jackrabbit) @f2bb185.
- **N12:** iiaiiappa/lt104-keyboard-linux (master, 2026-11-02).
- **N13:** regenbildr TH99 Pro @53b3678 README:70-72.
- **Web**
  - **W1:** AK980 V2 ekranlı klavye kılavuzu (manuals.plus kopyası; site 403 verdi, yalnızca arama özeti okundu). https://manuals.plus/ajazz/ak980-v2-tri-mode-with-screen-mechanical-keyboard-manual
  - **W2:** MechLands AK980 V2 Amazon ürün metni. https://www.amazon.com/dp/B0DF7SM7KH
  - **W3:** Sina incelemesi. https://k.sina.cn/article_1662256934_63140726001017zku.html
  - **W4:** Cubed3 incelemesi. https://www.cubed3.com/tech/reviews/accessories/ajazz-ak980-98-wired-mechanical-gaming-keyboard
  - **W5:** AJAZZ sürücü listesi. https://www.a-jazz.com/en/h-col-159.html
