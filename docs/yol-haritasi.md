# Keymotion yol haritası: ilk sürüme kadar

Durum: 2026-11-12 (sabah: Aşama 0–4 bitti; sıradaki Aşama 5). Kaynaklar: kullanıcının kararları, `docs/DURUM.md` madde 38 ve 42, `docs/urun-eksikleri.md`.

**Sıra değişti (kullanıcının kararı, 2026-11-12):** kod imzalama başvurusu (eskiden 1b, şimdi Aşama 9) ilk sürümden sonraya kaydı. SignPath Foundation başvurmadan önce yayınlanmış bir sürüm, belgelenmiş bir indirme sayfası ve projenin kullanıldığına dair kanıt istiyor (Aşama 9). Aşama 7 de genişledi: GitHub sayfası, bizim belge notlarımız ve SignPath'in sayfa şartları tek aşamada, uçtan uca yapılacak.

Bu belge kalan işlerin **sırasını** tutar. Her aşamanın ayrıntısı ve kararları da burada. Bir aşama bitince "Durum" sütununu ve DURUM'daki "Kalınan yer" satırını güncelle.

## Sıralama ilkeleri

1. **Önce uygulamanın temel davranışları, sonra paketleme.** Kurulum dosyası, Windows'la başlatma kaydı, kısayollar, imza ve güncelleyici; exe adına ve pencerenin yaşam döngüsüne bağlı. Bunlar sonradan değişirse kullanıcıların bilgisayarında eski kayıtlar kalır.
2. **Dışarıdan bekleme süresi olan işler, şartları sağlanır sağlanmaz başlar.** Kod imzalama başvurusu yayınlanmış bir sürüm ve kullanım kanıtı ister; bu yüzden ilk sürümden sonra yapılır (Aşama 9). İmza beklenirken hiçbir iş durmaz: kurulum dosyası, sürüm hattı ve belgeler imzadan bağımsızdır.
3. **Ekran görüntüleri ve belgeler sona yakın.** Arayüz kesinleşmeden çekilen görüntüler eskir. Belgeler (Aşama 7) kurulum dosyası ve sürüm hattı hazır olunca yazılır, çünkü kurulum ve indirme adımlarını anlatırlar.
4. **Klavyeye yazan her deneme önce kullanıcıya sorulur.** Ne yazılacağı kısaca söylenir.
5. **Her aşama aynı biçimde biter:**
   - denetimler: cargo fmt/clippy/test, tsc, ESLint, `tools/check-*.mjs`;
   - gerçek uygulamada doğrulama (CDP);
   - belgeler;
   - PR'ın `develop`'a birleşmesi.
6. Yardımcı ajanlar kendi worktree'lerinde çalışır. Başlattıkları sunucuları kapatır (ders 55). İşleri birleşince klasörleri silinir.

## Özet

| Sıra | Aşama | Bağımlılık | Kullanıcıdan gereken | Durum |
|---|---|---|---|---|
| 0 | Açık işleri kapat (lint/mock yardımcısı, PR #58) | — | — | Bitti (PR #58, #59) |
| 1 | Rust paket ve exe adı `keymotion` | 0 | Onay (öneri: evet) | Bitti (PR #60) |
| 2 | Tepsi ve arka plan | 1 | Karar verildi: tepsi | Bitti (PR #61): kullanıcı 1–8, ben 9–13 denedik |
| 3 | Keymotion açılış animasyonu | — (2 ile paralel olabilir) | Önizlemeye onay | Bitti (PR #62; kullanıcı A'yı, yazılı seçti) |
| 4 | Sorun bildirme ve GitHub sorun formları | 2 | Özel güvenlik bildirimi ayarı için onay | Bitti (PR #63); GitHub ayarları kullanıcı onayıyla açıldı |
| 5 | Kurulum dosyası (NSIS), imzasız | 1, 2 | Kurulum ayrıntılarına onay | Yapıldı (PR'da); kullanıcının elle denemesi ve kurulum görselleri bekliyor |
| 5c | Kurulum denemesi bulguları (K1–K11): simge ve görseller, GitHub bağlantıları, hatırlatıcı kartı, ekran durumu, odak zamanlayıcısı, bildirim, Genel bakış | 5 | Kararlar verildi: K9 her açılışta bir kez; K2, K7, K11 için A | Yapıldı (PR #72–#77); klavyede gerçek yüklemeyle deneme kullanıcıda |
| 5d | Kullanıcının geri bildirimleri (K12 Genel bakış'taki klavye çizimi, K13 açılış logosunun keskinliği) ve denemeleri | 5c | — | K12 yapıldı (A); K13 kapandı (kod değişikliği gerekmedi); A denemeleri yapıldı, K14 düzeltildi; B kurulum derlemesiyle |
| 6 | Sürüm hattı (GitHub Actions) ve otomatik güncelleyici | 5 | Güncelleme anahtarını GitHub Secrets'a koyma onayı | Kod yapıldı; kalan: anahtarı Secrets'a koymak, iş akışını denemek, rc.1 → rc.2 güncelleme denemesi |
| 6b | Uygulama penceresinin açılış animasyonu | 5 (kurulu sürümle ölçülür) | Seçim A (2026-11-12) | Yapıldı (PR #78); kurulu sürümde açılış süresi ölçülecek |
| 7 | GitHub sayfası ve bütün belgeler, SignPath'in sayfa şartları dahil | 5, 6 (kurulum ve indirme adımları belli) | Belge yapısına ve görünümüne onay (önce önizleme) | Bekliyor |
| 8 | Son denemeler ve ilk sürüm 0.1.0 (imzasız) | hepsi | Elle denemeler; `main`'e birleştirme onayı | Bekliyor |
| 9 | Kod imzalama: SignPath Foundation başvurusu ve imzalı sürüm (eskiden 1b) | 8 ve kullanım kanıtı | Başvuruyu kullanıcı yapar; formu birlikte doldururuz | İlk sürümden sonra |

## Aşama 0: açık işleri kapat

- **Lint ve sahte arka uç yardımcısı:**
  - 15 ESLint uyarısı davranış değişmeden düzeltilecek.
  - ESLint CI'da engelleyici olacak (sıfır uyarı).
  - Ders 40 güncellenecek.
  - Sahte arka uç (`?mock`, `?fw`, `?reset`, `?failPart`) yalnızca geliştirme sürümünde kalacak.
  - Yardımcının raporu gelince her uyarı için yapılanı incele. Üretim derlemesinde sahte kodun kalmadığının kanıtına bak. Sonra birleştir.
- **PR #58:** repo adı, İngilizce varsayılan, uyarı günlüğü, belgeler, notices. Otomatik birleşmede.

## Aşama 1: Rust paket ve exe adı

- **Öneri:** `klavye-studio` → `keymotion`.
  - Kurulu exe `keymotion.exe` olur; Görev Yöneticisi'nde ve Windows'la başlatma kaydında bu ad görünür.
  - Kütüphane adı `keymotion_lib`. Paket ve kütüphane aynı adı taşırsa Windows'ta `.pdb` dosyaları çakışır.
- **Neden şimdi:** Windows'la başlatma kaydı (Aşama 2), kısayollar, imza (Aşama 5) ve güncelleyici (Aşama 6) exe adına bağlı.
- **Dokunulacak yerler:**
  - `app/src-tauri/Cargo.toml` (paket ve lib adı), `main.rs` (`klavye_studio::`);
  - `tauri.conf.json` (`mainBinaryName` gerekirse);
  - `README.md`, `docs/DURUM.md` §5 (`target/debug/klavye-studio.exe`), CI;
  - `tools/cdp.mjs` notları, yardımcı brifleri.
- **Değişmeyenler:** veri klasörleri kimliğe bağlı (`dev.keymotion`), taşıma gerekmez. `migrate.rs` dokunulmaz.
- **Doğrulama:** temiz derleme (`cargo clean -p` ile, ders 50), uygulama açılıyor, ayarlar yerinde.

## Aşama 2: tepsi ve arka plan

**Kullanıcının kararı (2026-11-12):** pencere kapatılınca uygulama tepside, yani saatin yanındaki simge alanında çalışmaya devam eder. Önerilen ayrıntılar kabul edildi.

### Neden

- **Klavyede kalanlar:** ışıklar, tuş atamaları, makrolar ve ekran görseli klavyenin kendi belleğinde. Uygulama kapalıyken de çalışırlar.
- **Uygulamaya bağlı olanlar:** bunlar uygulama çalışırken iş görür; bugün pencere kapanınca hepsi duruyor:
  - odak zamanlayıcısı;
  - hatırlatıcılar;
  - takvim kartları;
  - canlı ekran (saat, çalan şarkı);
  - tuşlardaki sistem göstergeleri;
  - klavye saatinin eşitlenmesi.

### Davranış

- **Kapatma:** X ya da Alt+F4 pencereyi gizler, uygulama tepside çalışır.
  - Ayar: "Kapatınca tepside çalışmaya devam et", varsayılan **açık**.
  - Kapalıysa bugünkü gibi tamamen kapanır.
- **İlk gizleme:** yalnızca bir kez Windows bildirimi çıkar: "Keymotion arka planda çalışıyor. Simgesi saatin yanında." Bildirim `notify.rs` ile gönderilir.
- **Tepsi simgesi:**
  - Sol tık pencereyi gösterir ve öne getirir.
  - Araç ipucu "Keymotion". Odak sürüyorsa kalan süre de görünür.
- **Sağ tık menüsü** (arayüz dilinde; metinler `tray_texts` ile arka uca iletilir). Kullanıcının ilk denemesinden sonra (2026-11-12) alt menülü düzene geçildi:
  - en üstte klavye satırı, tıklanmaz: "AJAZZ AK980 PRO · USB kablo", kablo yoksa pil yüzdesiyle; klavye yoksa "Klavye bağlı değil";
  - Keymotion'ı aç;
  - Odak zamanlayıcısı ▸
    - üstte, tıklanmaz: aşama, tur ve kalan dakika ("Odak · Tur 2/4 · 18 dk kaldı", "Duraklatıldı · Kısa mola · 3 dk kaldı", boştayken "Hazır: 4 × 25 dk odak");
    - Başlat / Duraklat / Sürdür (duruma göre);
    - Sonraki aşamaya geç;
    - Durdur.
    - Sonraki aşamaya geç ve Durdur yalnızca zamanlayıcı çalışırken ya da duraklatılmışken kullanılır.
  - Hatırlatıcılar ▸ sıradaki hatırlatıcı ("Sıradaki: 14:30 · başlık", "yarın 09:00"), "Hatırlatıcılar açık" işareti;
  - Tuşlarda sistem göstergeleri (işaretli/işaretsiz; yalnızca RAM, flash'a yazmaz);
  - Profiller ▸ kayıtlı profiller, klavyede olan işaretli;
  - Çık.
  - Menü bir kez kurulur, sonra yalnızca maddelerin yazısı ve işaretleri değişir. Bu yüzden açık menü kapanmaz, kalan dakika menü açıkken de güncellenir (10 sn'de bir).
  - **Profil seçimi** klavyeye yazar. Bu yüzden pencereyi açar ve Profiller sayfasında o profilin onay satırını gösterir. Onaysız yükleme yok; bu, "her adımda Vazgeç" kuralına uyar.
- **Çık:** kaydedilmemiş değişiklik ya da süren bir yazma varsa pencere açılır ve mevcut kapatma penceresi gösterilir (`useCloseGuard`, `UnsavedDialog`). Yoksa uygulama kapanır.
- **İkinci açılış:** gizli pencereyi gösterir. Tek örnek eklentisi zaten var; `show()` eklenir.
- **Windows kapanırken ya da oturum kapanırken:** normal çıkış. Yazma sürüyorsa mevcut `ShutdownBlockReasonCreate` devrede.

### Windows açılınca başlatma

- Ayar: "Windows açılınca başlat", varsayılan **kapalı**.
- Açılırsa `tauri-plugin-autostart` kullanılır (HKCU `Run` kaydı). Uygulama `--hidden` argümanıyla başlar ve pencere gösterilmez, yalnızca tepsi simgesi çıkar.
- Kaldırıcı bu kaydı siler (Aşama 5).

### Ayarlar

- Ayarlar sayfasında yeni "Arka plan" kartı, iki anahtarla:
  - "Kapatınca tepside çalışmaya devam et";
  - "Windows açılınca başlat".
- Kartta kısa bir açıklama: hangi özellikler arka planda çalışır.
- Ayarlar arka uçta saklanır (örneğin `app.json`, `settings.rs` kalıbıyla).

### Teknik riskler ve ölçümler

- **Kartlar gizli pencerede:** canlı kartlar WebView'da çiziliyor.
  - Arka uç olayları gönderir: `calendar-card`, `pomodoro-card`, çalan şarkı. Arayüz kartı çizer ve `live_submit` ile geri yollar.
  - Pencere gizliyken WebView2 sayfayı "görünmez" sayar: `requestAnimationFrame` durur, zamanlayıcılar kısılır. Olay dinleyicileri çalışır ama çizim yolu rAF ya da yazı tipi yüklemesi beklerse takılır.
  - **Ölç:** pencere gizliyken saat, odak ve takvim kartı zamanında yükleniyor mu? Günlükte `live` satırlarına bak.
  - **Gerekirse:** çizimi rAF'siz yap, ya da WebView2 argümanları ekle: `--disable-background-timer-throttling`, `--disable-renderer-backgrounding`, `--disable-backgrounding-occluded-windows`.
- **Bellek ve işlemci:** gizli WebView yaklaşık 150–250 MB bellekte kalır.
  - Ölç: `Get-Process`.
  - Ayna okumasını (`04 F5`) yalnızca Genel Bakış görünürken yap. Bugün sürekli okuyor ve ara sıra reddediliyor.
  - Odak, göstergeler ve hatırlatıcılar zaten arka uçta.
- **Güç kullanımı** burada ölçülür: boşta, odak sürerken, göstergeler açıkken. Windows "Güç kullanımı" ve işlemci zamanıyla.
- **Takma ve çıkarma:** bugün klavyeler düzenli taramayla bulunuyor. Arka planda güç için `CM_Register_Notification` bildirimine geçmeyi değerlendir (DURUM P3).

### Ölçümler (2026-11-12, debug sürüm)

- **Gizli pencerede kısılma yok.** Tauri pencereyi `SW_HIDE` ile gizliyor, WebView2'ye "görünmez" denmiyor:
  - `document.visibilityState` "visible" kalıyor;
  - 100 ms'lik zamanlayıcı 1,5 sn'de 15 kez çalıştı;
  - `requestAnimationFrame` çalıştı;
  - örnek hatırlatıcı kartı 137 ms'de çizildi.
  - Bu yüzden WebView2 argümanı eklenmedi.
  - **6 dakika 20 saniye gizli kaldıktan sonra** (Chromium'un 5 dakikalık yoğun kısma eşiğinin ötesi; senaryo 13) yine aynı: sayfa "visible", zamanlayıcı 1,5 sn'de 15 kez, `requestAnimationFrame` çalıştı, kart 457 ms'de çizildi.
- **Ayna okuması (`04 F5`) pencere gizliyken duruyor:** `useMirror` artık `windowShown`'a bağlı.
- **Bellek:** pencere gizliyken 7 süreç, toplam çalışma kümesi yaklaşık 270 MB (debug sürüm, hata ayıklama portu açık). Kurulu sürümde yeniden ölçülecek.
- **Bellek, 6 dk gizli kaldıktan sonra:** çalışma kümesi 170 MB (Windows boştaki sayfaları geri aldı), özel bellek 317 MB.
- **İşlemci:** 6 dk 20 sn gizliyken toplam 5,6 sn işlemci zamanı, yani tek çekirdeğin ortalama yaklaşık %1,5'i. Bu ölçüm debug sürümde, hata ayıklama portu açıkken alındı; kurulu sürümde Görev Yöneticisi › Güç kullanımı ile yeniden bakılacak.

### Doğrulama (gerçek uygulama)

**Yapılanlar (2026-11-12, CDP ile):**
- kapat → gizlen, ilk bildirim bir kez (`background.json` `noticeShown`);
- ikinci açılış pencereyi gösteriyor;
- gizli açılış (`--hidden`) ve sonra ikinci açılış;
- arayüz `windowShown`'ı izliyor;
- Ayarlar › Arka plan › "Windows açılınca başlat": `Run` kaydı `"…\keymotion.exe" --hidden` olarak yazıldı ve kapatınca silindi;
- tepsiden profil seçmenin olayı Profiller sayfasında onay satırını açtı, "Vazgeç" kapattı.

**Kullanıcının denediği (ilk menü):** odak başlat/duraklat çalıştı, ama bitirme yoktu. Bunun üzerine alt menülü düzene geçildi.

**Kullanıcının denediği (alt menülü sürüm):** aşağıdaki 1–8 tamam.
- Bildirim "Windows PowerShell" adı ve simgesiyle geldi. Düzeltildi, aşağıda "Bildirimler".
- "İkinci açılış İngilizce ve simgesiz" gözlemi: günlükte o saatte uygulama açılışı yok. Açılan, Claude uygulamasının önizleme panelindeki tarayıcı kopyasıydı (`ui`, 1420): arka ucu yok, İngilizce başlar, tepsisi yok (DURUM ders 57).

**Benim denediklerim (9–13, CDP ile, fare kullanmadan):**
- **9:** "Kapatınca tepside…" kapalıyken X uygulamayı tamamen kapattı.
- **10:** Windows'la başlatma:
  - açınca `Run` kaydı `--hidden` ile yazıldı;
  - Görev Yöneticisi'nin kapatma işareti (`StartupApproved\Run`, 03…) yazılınca Ayarlar'da anahtar kapalı göründü ve "Görev Yöneticisi'nde kapatılmış" notu çıktı;
  - Ayarlar'dan yeniden açınca işaret silindi;
  - kapatınca kayıt kalktı.
- **11:** ikinci açılış gizli pencereyi getirdi, tek süreç kaldı.
- **12:** dil İngilizceye geçince tepsi menüsü de geçti (günlük: `tray menu language locale=en`), Türkçeye dönünce geri döndü.
- **13:** 6 dk 20 sn gizli kaldıktan sonra kısılma yok (kullanıcı 10 dakikayı beklemeye gerek görmedi). Ayrıntı "Ölçümler"de.

### Bildirimler (2026-11-12)

**Kimlik:** Windows bildirimi uygulamanın kimliğiyle (AUMID) gösterir.
- Kurulu sürümde kimliği Başlat menüsü kısayolu verir. Geliştirme sürümünde kısayol yok; bu yüzden PowerShell'in kimliği ödünç alınıyordu ve bildirim "Windows PowerShell" adıyla çıkıyordu.
- Artık uygulama açılışta kendi kimliğini (`dev.keymotion`) kaydediyor: `HKCU\Software\Classes\AppUserModelId\dev.keymotion`, `DisplayName` "Keymotion" ve `IconUri` (logo, `%LOCALAPPDATA%\dev.keymotion\notification-icon.png`). Kısayol gerekmiyor.
- Kayıt olmazsa geliştirme sürümü PowerShell'in kimliğine düşer.
- Doğrulama: arka plan bildirimi Windows'un bildirim geçmişinde `dev.keymotion` altında göründü.
- Bildirim başlığında uygulama adı zaten görünüyor. Bu yüzden metin "Arka planda çalışmaya devam ediyor" oldu, ad tekrarlanmıyor.
- **Metin yeniden yazıldı (2026-11-12, kullanıcı "çok kötü ve saçma görünüyor" dedi):** başlık "Arka planda çalışıyor" / "Still running"; açıklama tek cümle: "Yeniden açmak için saatin yanındaki Keymotion simgesine tıklayın." / "To open it again, click the Keymotion icon next to the clock." ^ oku ve Çık anlatımı bildirimden çıktı; Ayarlar'daki açıklamada duruyor.

**Uygulamanın gönderdiği Windows bildirimleri (tam liste):**
1. **Arka planda çalışıyor:** pencere ilk kez tepsiye gittiğinde, bir kez (bilgisayar başına; `background.json` `noticeShown`). Sonraki kapatmalarda yok.
2. **Hatırlatıcı:** bir hatırlatıcı çaldığında başlığı ve notu. Hangi durumda çıkar:
   - "Windows bildirimi de gönder" ayarı açıksa (varsayılan kapalı) her seferinde;
   - klavye USB kablosunda değilse her zaman, çünkü tuşlar yanamaz.
   - Bildirimin kendi sesi yok; hatırlatıcının sesi açıksa Windows sesi ayrıca çalar.
- **Bildirim göndermeyenler:**
  - odak zamanlayıcısı (aşama değişince ses, tuşlar ve ekran kartı);
  - takvim (toplantı öncesi tuş yanıp söner ve ekran kartı çıkar);
  - hatalar ve uyarılar (yalnızca uygulama içinde).
- **Karar (kullanıcı, 2026-11-12): böyle kalsın.** Klavye kablosuzken her hatırlatıcı Windows bildirimi gönderir, çünkü tuşlar yanamaz ve hatırlatıcının ulaşmasının tek yolu bu. Arka plan bildirimi yalnızca ilk seferde çıkar.

**Elle denenecekler** (kullanıcıya verilen senaryolar; simge saatin yanındaki ^ içinde):

1. **Kapatma:** X → pencere kaybolur, simge kalır, "Keymotion arka planda çalışıyor" bildirimi bir kez çıkar.
2. **Sol tık** simgeye → pencere geri gelir.
3. **Sağ tık** → en üstte klavye satırı ("AJAZZ AK980 PRO · USB kablo").
4. **Odak zamanlayıcısı ▸:**
   - boştayken "Hazır: …";
   - Başlat → aşama satırı ve kalan dakika, fare simgenin üstündeyken araç ipucunda da;
   - Duraklat → "Duraklatıldı · …", Sürdür;
   - Sonraki aşamaya geç → mola;
   - Durdur → "Hazır";
   - menü açıkken dakika azalır.
5. **Hatırlatıcılar ▸:** sıradaki hatırlatıcı. "Hatırlatıcılar açık" işareti kaldırılınca "Hatırlatıcılar kapalı" görünür ve sayfadaki anahtar da kapanır. Sonra tekrar aç.
6. **Tuşlarda sistem göstergeleri:** işaretlenince tuşlarda çubuklar çıkar, Sistem göstergeleri sayfasındaki anahtar da açılır. Sonra kapat.
7. **Profiller ▸:** klavyedeki profil işaretli. Başka biri seçilince pencere açılır ve Profiller'de onay satırı görünür; Vazgeç.
8. **Çık:**
   - değişiklik yokken: uygulama kapanır, simge gider;
   - kaydedilmemiş değişiklikle (örneğin tuş başına bir renk): X pencereyi gizler, değişiklik kaybolmaz. Tepsiden Çık → pencere açılır ve "kaydedilmemiş" penceresi çıkar. Vazgeç → uygulama açık kalır, X yine gizler.
9. **Ayar kapalıyken:** Ayarlar › Arka plan › "Kapatınca tepside…" kapalı → X uygulamayı tamamen kapatır. Sonra ayarı geri aç.
10. **Windows'la başlatma:**
    - açınca Görev Yöneticisi › Başlangıç uygulamaları'nda "keymotion" görünür;
    - orada devre dışı bırakılınca Ayarlar'daki anahtar kapalı görünür ve "Görev Yöneticisi'nde kapatılmış" notu çıkar;
    - sonra kapat.
    - Debug sürüm arayüzü Vite'tan yüklediği için gerçek yeniden başlatma kurulu sürümde denenecek.
11. **İkinci açılış:** pencere gizliyken exe'yi yeniden çalıştır → aynı pencere gelir, ikinci simge çıkmaz.
12. **Dil:** Ayarlar'da English → menü İngilizce olur.
13. **Uzun gizli kalma (10 dk ve üstü):** hatırlatıcı ve odak kartları zamanında gelir. Bu, Chromium'un kısma eşiği için.

- kapat → gizlen;
- ilk bildirim;
- tepsi menüsünün her maddesi;
- ikinci açılış;
- Çık: hem kapatma penceresiyle hem pencere olmadan;
- gizliyken kartların ve hatırlatıcının gelmesi;
- Windows'la başlatma: kayıt oluşuyor ve siliniyor, uygulama gizli başlıyor;
- bellek ve güç ölçümleri.

## Aşama 3: Keymotion açılış animasyonu

- **Amaç:** fabrika animasyonunun yedeği olmayanların GIF sayfasına koyabileceği özgün görsel. AJAZZ'ın animasyonu paketlenmez (karar).
- **Konsept:** logodaki üç çizgili "K". Çizgiler sırayla belirir, sağ alttaki turuncu çizgi yanar ve hafifçe parlar. Koyu zemin. 240×135, en çok 140 kare, döngülü.
- **Akış:**
  1. sohbette etkileşimli önizleme ve kullanıcının onayı;
  2. karelerin kodla üretilmesi (tekrarlanabilir);
  3. kütüphaneye yerleşik "Keymotion" öğesi olarak eklenmesi;
  4. fabrika yedeği yoksa "Fabrika animasyonunun yedeği yok" uyarısında öneri olarak sunulması.
- **Ne zaman:** Aşama 2 ile paralel yürüyebilir (ayrı yardımcı). Ekran görüntülerinden (Aşama 7) önce bitmeli.

### Yapılan (2026-11-12)

- **Önizleme ve seçim:** sohbette iki seçenek gösterildi: A "Çizerek" ve B "Işık süpürmesi", ikisi de yazılı ya da yazısız. Gerçek kare sayısı ve yükleme süresi önizlemedeydi. **Kullanıcı A'yı, "keymotion" yazısıyla seçti.**
- **Animasyon** (`app/src/lib/bootAnimation.ts`):
  - sıra: dikey çizgi iner, üst kol uzar, turuncu bacak çizilir, turuncu çizgi parlar, "keymotion" soldan açılır ("key" krem, "motion" turuncu, uygulamanın Bricolage yazı tipi), 2 sn bekler, söner;
  - 37 kare, döngü 3,7 sn, yükleme yaklaşık 1 dk 25 sn (586 sayfa);
  - düz renkler, titreşimsiz (dither yok) RGB565.
  - Az kare bilinçli: her kare yaklaşık 2,3 sn yükleme ve flash aşınması demek. Uzun duruşlar aynı karenin uzun gecikmesiyle yapılıyor (en çok 508 ms).
- **Kütüphane:** yeni öğe türü `keymotion` (yerleşik).
  - Kütüphanede tek; silinemez, yeniden adlandırılamaz.
  - Sırada fabrika yedeğinden sonra gelir.
  - Adı arayüzde çevrilir: "Keymotion animasyonu" / "Keymotion animation". Etiketi "Uygulamayla gelir".
  - Arayüz kütüphaneyi ilk yüklediğinde öğe yoksa kareleri çizip ekler. Klavyeye hiçbir şey yazılmaz; yüklemek kullanıcının işi ve yine onay ister.
  - Aynı resim sıradan bir öğe olarak kayıtlıysa yerleşik öğeye dönüşür.
  - Yedeğe girmez (uygulama zaten çizer). Yedekten ya da başka bilgisayardan gelen profil, buradaki öğeyi kullanır.
- **Öneri:** "Fabrika animasyonunun yedeği yok" uyarısının altında "Onun yerine Keymotion animasyonunu koyabilirsiniz; kütüphanede hazır." ve "Kütüphanede göster" bağlantısı var. Bağlantı öğeyi gösterip "Yükle" düğmesine odaklanıyor.
- **Denemeler:**
  - gerçek uygulamada öğe ekleniyor, kareler doğru (altı karenin görüntüsüne bakıldı), düğmeler doğru;
  - öneri tarayıcı önizlemesinde (fabrika yedeği yok) çalışıyor;
  - Rust testi: tek öğe, silinemez, adı değişmez, sıradan kopya dönüşür.

## Aşama 4: sorun bildirme ve GitHub sorun formları

Kararlar: yalnızca GitHub, e-posta yok. Uygulama kendiliğinden hiçbir şey göndermez. Ayrıntı: `docs/urun-eksikleri.md` bölüm I.

### 4.1 GitHub tarafı (önce)

Uygulama içi düğme bu formların adresini açacak, o yüzden formlar önce yapılır.

- **Sorun formları** `.github/ISSUE_TEMPLATE/`: hata, klavye desteği isteği, öneri. İngilizce, kısa Türkçe açıklamalı.
- **`config.yml`:** soruları Discussions Q&A'ya yönlendirir.
- **`SECURITY.md`:** güvenlik açıkları için GitHub'ın "Private vulnerability reporting" özelliği. Bu bir repo ayarı; açmadan önce kullanıcıya sor.

### 4.2 Uygulama tarafı

- **Beklenmeyen hata:** sakin bir bildirim ve "Sorunu bildir" düğmesi. Aynı hata tekrar gösterilmez. Beklenen durumlar, yani `errors.ts` `EXPECTED`, bildirim istemez.
- **Çökme:** sonraki açılışta bir kez "geçen sefer beklenmedik şekilde kapandı" ve "Bildir". Kaynak `crash-*.txt`.
- **Bildirme penceresi:**
  - gönderilecek metni önce gösterir (tanılama bilgisi, son günlük satırları);
  - "GitHub'da bildir" formu açar (`open_link`, `PROJECT_PAGES` izin listesi) ve ayrıntıyı panoya kopyalar;
  - "Kopyala" ayrıca bulunur.
- **Desteklenmeyen klavye:**
  - Tetikleyici: VID 0C45 (SONiX) ya da AJAZZ ürün adı taşıyan ama listede olmayan bir klavye.
  - "Bu klavye henüz desteklenmiyor — destek iste" düğmesi çıkar.
  - Bildirime VID/PID, ürün adı, arayüzler ve HID tanımlayıcıları eklenir. Yalnızca okunur, klavyeye bir şey yazılmaz.
  - Form: "klavye desteği".

### Yapılan (2026-11-12, sabah)

- **GitHub tarafı:**
  - formlar `.github/ISSUE_TEMPLATE/`: `bug.yml` (sorun), `keyboard.yml` (klavye desteği), `idea.yml` (öneri). İngilizce, "Türkçe yazabilirsiniz" notuyla. Ayrıntılar kutusunun kimliği `details`; klavye formunda model alanı `model` (uygulama doldurur);
  - `config.yml`: boş sorun kapalı; sorular Discussions Q&A'ya, güvenlik sorunları özel bildirime yönlendirilir;
  - `SECURITY.md`: GitHub'ın özel bildirim formu, ilk yanıt bir hafta içinde (tek kişilik proje), kapsam ve kapsam dışı (klavye yazılımı ve AJAZZ'ın programı).
  - Formlar varsayılan daldan (`develop`) okunur; PR birleşince canlı olur.
- **İki GitHub ayarı** (kullanıcı onayladı, 2026-11-12):
  - "Private vulnerability reporting" açıldı; `SECURITY.md`'deki bağlantı çalışıyor;
  - "keyboard support" etiketi oluşturuldu; klavye formu bu etiketi koyar.
- **Uygulama tarafı:**
  - **Sorun bildir penceresi** (`components/Reporting.tsx` `ReportDialog`): gönderilecek metni önce gösterir (tanılama bilgisi, en üstte "What happened"). "GitHub'da bildir" metni panoya kopyalar ve formu başlıkla açar (`open_issue`; adres Rust'ta kurulur, başlık ve model kodlanır, başka adres açılamaz). "Kopyala" ayrıca var. Uygulama kendiliğinden hiçbir şey göndermez.
  - **Ayarlar › Hakkında › Geri bildirim:** "Sorun bildir" ve "Öneri gönder" (öneri formu doğrudan açılır).
  - **Beklenmeyen hata:** `errors.ts` `EXPECTED` dışındaki bir hata sağ üstte sakin bir bildirim açar ("Beklenmeyen bir sorun oldu", "Sorunu bildir"). Aynı hata (sayılar hariç aynı metin) oturumda bir kez gösterilir; bildirim 20 sn sonra kapanır, fare üstündeyken beklemez. Kaydetme çubuklarını örtmemesi için sağ üstte.
  - **Çökme:** sonraki açılışta son 7 günün en yeni, daha önce önerilmemiş çökme notu bir kez önerilir ("Keymotion geçen sefer bir sorunla karşılaştı", "Bildir"). İşaret günlük klasöründe `crash.seen`; bu çalışmada yazılan notlar "geçen sefer" sayılmaz.
  - **Sayfa çizim hatası** (ErrorBoundary): "Tanılama bilgisini kopyala" yerine "Sorunu bildir"; pencerenin tamamı çökse de kendi penceresini açar.
  - **Desteklenmeyen klavye:** SONiX kimliği (VID 0C45) ya da adında "AJAZZ" olan, standart klavye koleksiyonu bulunan ama hiçbir modelle eşleşmeyen aygıt (`kb_core::unknown_keyboards`; SONiX kameraların düğmeleri sayılmaz). Hoş geldiniz ekranında "Bu klavye henüz desteklenmiyor" kartı ve "Destek iste". Metinde USB kimlikleri, adlar, koleksiyonlar ve her birinin HID tanımı var; tanımlar Windows'tan okunur (`report_descriptor_at`), klavyeye bir şey gönderilmez. Seri numarası ve aygıt yolları eklenmez.
- **Denemeler:**
  - gerçek uygulama (CDP): Ayarlar'daki satır ve pencere, kopyalama, Esc ile kapanma; beklenmeyen hata bildirimi bir kez çıkıyor, beklenen hata çıkmıyor; eski tarihli bir deneme çökme notu bir kez önerildi, raporda notun içeriği vardı, sonra önerilmedi (deneme dosyaları silindi); "GitHub'da bildir" tarayıcıda sayfa açtı, hata yok;
  - AK980 PRO'nun 8 koleksiyonunun hepsinin tanımı okunuyor, klavye koleksiyonu dahil (`kb-transport` donanım testi, `--ignored`);
  - desteklenmeyen klavye kartı ve "Destek iste" tarayıcı önizlemesinde (`?mock=none&unsupported`); `?crash` sahte çökme notu verir;
  - Rust testleri: adres kodlama, çökme notu seçimi ve işareti, rapordaki "What happened", klavye ayrıntıları (yollar dışarıda), bilinmeyen klavye tanıma.
- **Bilinen sınır:** kart yalnızca hoş geldiniz ekranında; desteklenen bir klavye de takılıysa desteklenmeyen için kart çıkmaz.

## Aşama 5: kurulum dosyası (NSIS), imzasız

- **Kurulum:**
  - kullanıcı başına kurulum, yönetici izni gerekmez;
  - Başlat menüsü kısayolu; masaüstü kısayolu isteğe bağlı;
  - kurulum dili İngilizce ve Türkçe;
  - WebView2 önyükleyicisi.
- **Kaldırıcı:**
  - "ayarlarımı da sil" seçeneği, varsayılan: sakla;
  - Windows'la başlatma kaydını siler (`Run` ve `StartupApproved\Run` altındaki "Keymotion");
  - bildirim kimliğini siler (`HKCU\Software\Classes\AppUserModelId\dev.keymotion`);
  - çalışan uygulamayı kapatır.
- **Exe bilgileri:** ürün adı, açıklama, sürüm, simge.
  - Ürün adı her dosyada "Keymotion", sürüm numarası uygulamada ve kurulum dosyasında aynı (SignPath şartı: ürün adı proje adı, sürüm her derlemede tutarlı).
- **SignPath'in kurulumla ilgili şartları** (Aşama 9; şimdiden uyulur):
  - kaldırma imkânı (Windows'un "Yüklü uygulamalar" listesinden);
  - sisteme yapılan değişiklikler kullanıcıya söylenir: "Windows açılınca başlat" zaten Ayarlar'da, kullanıcının seçimi;
  - uygulama veri toplamaz; gizlilik cümlesi belgelerde (Aşama 7).
- **İmza:** bu aşamada yok. Kurulum dosyası imzasız çıkar; imza Aşama 9'da sürüm hattına eklenir, kurulum dosyasında değişecek bir şey olmaz.
- **Doğrulama:**
  - temiz bir Windows kullanıcı hesabında ya da sanal makinede kur, aç, kaldır;
  - SmartScreen davranışı (imzasız): "Windows bilgisayarınızı korudu" ekranı ve "Ek bilgi" → "Yine de çalıştır" yolu; sürüm notunda anlatılacak;
  - kurulu sürümde sahte arka uç ve geliştirici anahtarları olmamalı (Aşama 0).

### Yapılan (2026-11-12, sabah)

- **Ayarlar** (`app/src-tauri/tauri.conf.json` `bundle`):
  - NSIS, kullanıcı başına (`currentUser`): `%LOCALAPPDATA%\Keymotion`, yönetici izni yok;
  - diller İngilizce ve Türkçe; Windows'un diline göre seçilir, Türkçe değilse İngilizce;
  - yayımcı "Keymotion", telif satırı, ana sayfa (GitHub), lisans "MIT OR Apache-2.0", kısa ve uzun açıklama;
  - WebView2: varsayılan (gerekirse önyükleyiciyi indirir; Windows 11'de zaten var).
- **Tauri'nin şablonunda zaten olanlar** (2.12.4, `installer.nsi` okundu):
  - karşılama, klasör, kurulum ve bitiş sayfaları; bitişte "Masaüstü kısayolu oluştur" ve "Keymotion'ı çalıştır" kutuları;
  - kaldırıcıda "Uygulama verilerini sil" kutusu, varsayılan işaretsiz (veriler kalır);
  - çalışan uygulama varsa kapatmak için sorar (Windows'un yeniden başlatma yöneticisiyle);
  - `Run` altındaki "Keymotion" değerini kaldırırken siler; güncellemede dokunmaz.
- **Eklenen kaldırma adımı** (`app/src-tauri/windows/installer-hooks.nsh`): `StartupApproved\Run\Keymotion` ve `AppUserModelId\dev.keymotion` kaldırırken silinir, güncellemede kalır.
- **Derleme:** çalışan uygulama etkilenmesin diye ayrı bir worktree'de (`../klavye-build`), ortak `target` klasörüyle: `pnpm tauri build`. Çıktı `target/release/bundle/nsis/Keymotion_0.1.0_x64-setup.exe` (4 MB). İlk sürüm derlemesi yaklaşık 8 dk.
- **Denemeler (bu hesapta, sessiz kurulumla `/P`):**
  - kurulum: klasör, "Yüklü uygulamalar" kaydı (yayımcı, sürüm, GitHub bağlantıları), Başlat menüsü ve masaüstü kısayolu ✓;
  - exe bilgileri: ürün adı, açıklama, şirket, sürüm 0.1.0, telif ✓;
  - kurulu sürüm: arayüz uygulamanın içinden geliyor (`tauri.localhost`), geliştirme modülleri yok, klavye bulunuyor (yazılım 1.06) ✓;
  - kaldırma: klasör, kayıt, kısayollar ve bildirim kaydı silindi; ayarlar ve veriler kaldı ✓;
  - "Windows açılınca başlat" kaydının silinmesi elle denenmedi: kaydı betikle açmak otomatik izin denetimince engellendi. Kullanıcı deneyecek.
- **Bulunan hata (ders 61):** kurulu sürüm ilk açılışta, klavye takılıyken "Klavye bekleniyor" gösterdi. Arayüz klavye listesini okuyup değişiklik olayını sonra dinlemeye başlıyordu; hızlı açılan sürüm sürümünde olay arada kayboluyordu. Düzeltme `lib/follow.ts`: önce dinle, sonra oku; okuma sürerken olay gelirse okumanın cevabı atılır. Aynı kalıp saat kartında, flash yazma durumunda ve desteklenmeyen klavye listesinde de kullanıldı. Boş tercihlerle 6 açılışın 6'sında klavye göründü (düzeltmeden önce 2'de 1 kayboluyordu).
- **Bilinen:** kurulu sürüm geliştirme sürümünden ayrı bir adreste çalışır, arayüz tercihleri (dil, tema) ayrı tutulur. İlk açılışta İngilizce gelir (varsayılan dil kararı).
- **Kalan:**
  - kullanıcının elle denemesi: Türkçe kurulum sayfaları, masaüstü kısayolu kutusu, "Bilgisayar açılınca başlat" açıkken kaldırma;
  - kurulum görselleri: şu an NSIS'in varsayılan karşılama resmi ve üst bant; Keymotion görselleri önizlemeyle önerilecek;
  - SmartScreen davranışı ancak internetten indirilen dosyada görülür (Aşama 6).

## Aşama 5c: kurulum denemesi bulguları (2026-11-12, kullanıcının denemesi)

Kullanıcı kurulum dosyasını Explorer'dan, yani Claude'un kutusunun dışında denedi (ders 62) ve uygulamayı bir süre kullandı. Ekran görüntüleriyle bildirdiği her konu aşağıda bir iş olarak duruyor. Ekran görüntülerinin yeri hafızada (kişisel klasör yolu içerdikleri için repoya konmadı).

**Sıra (kullanıcının kararı):**
1. Bu listedeki hatalar, gerekirse yardımcı ajanlarla, hızlıca çözülür.
2. Sonra uygulamada kalan geliştirmeler: açılış animasyonu (6b), görseller, yapı.
3. Sonra yol haritasına dönülür.
4. Kurulum dosyası bu süreçte yeniden derlenmez; uygulama işleri bitince sonda bir kez alınır (hafıza: kurulum derlemesi sonda).

**Çalışanlar:**
- kurulum sayfaları Türkçe;
- uygulama Windows'un diliyle Türkçe açıldı;
- "Bilgisayar açılınca başlat" kaydı oluştu ve kaldırınca silindi;
- kısayollar ve "Yüklü uygulamalar" kaydı kaldırınca gitti;
- odak zamanlayıcısı genel olarak istendiği gibi çalışıyor;
- hatırlatıcının sesi ve ışık dalgası geliyor;
- Sorun bildir penceresi çalışıyor, açılan sayfa hariç (K4).

**Durum (2026-11-12, öğlen):**
- **Yapıldı, PR #72:** K1 (kurulum simgesi), K3 (kendi Türkçe ve İngilizce kurulum metinlerimiz, `windows/lang/*.nsh`), K4 (sayfalar `ShellExecuteW` ile tarayıcıda açılıyor), K9 (bildirim her açılışta bir kez). NSIS betiği `makensis` ile derlenerek denetlendi. Kurulum sayfaları ve tarayıcının açılması sondaki derlemede, kutunun dışında denenecek.
- **Denetlendi, K10:** Tauri'nin oluşturduğu betikte kutu işaretsiz oluşturuluyor (`BM_SETCHECK` yok). Ekran görüntüsündeki işaret kullanıcının tıklaması; gerçek AppData'daki `dev.keymotion` klasörleri bu yüzden silinmiş (denetlendi). Kutunun metni artık "Ayarlarımı ve kayıtlarımı da sil".
- **Yapıldı, PR #73:** K5 ve K8 (ayrıntılar maddelerin altında). Tarayıcı önizlemesinde denendi; klavyede gerçek yüklemeyle denenmedi (her yükleme flaşa yazar, kullanıcının denemesine kaldı).
- **K6:** hata değil; K7 ile ortadan kalkacak.
- **Seçildi ve yapıldı (2026-11-12, öğleden sonra):**
  - K2, seçenek A (sade): logo, keymotion yazısı ve "AJAZZ AK980 PRO" koyu zeminde (164×314), üst bantta beyaz zeminde logo ve yazı (150×57). `tools/installer-images.py` ile çizilir; dil yazısı olmadığı için iki dilde aynı. PR #75.
  - K7: kullanıcının kurduğu sıra. Odak ve mola blokları (1–180 dk, en çok 16), sürükleyerek ya da ok tuşlarıyla taşınır; hazır sıralar ("25 / 5 klasik" varsayılan: 25-5-25-5-25-15, "50 / 10 derin", boş); sıra bir kez, 2 kez, 3 kez ya da durdurulana kadar. Kartlar, tepsi satırı ve kenar çubuğu bloğu söyler. Eski ayarlar kurdukları sıraya dönüşür (kullanıcının 1/1/1 dk deneme ayarı "1 dk odak, 1 dk mola" oldu; "25 / 5 klasik"e bir tıkla geçilir). PR #76.
  - K11, seçenek A (kutulu kontrol paneli): klavye çizimi ve yanında klavye ekranındaki görsel; altında altı kutu: ışık, odak zamanlayıcısı, sıradaki hatırlatma, sıradaki toplantı, sistem göstergeleri (canlı çubuklar), profil. Hızlı renk değiştirme yok: renk Işıklar sayfasında kaydetmeyle (flaşa yazma) değişir. Yardımcı ajan ayrı worktree'de yaptı, odak zamanlayıcısına uyarlandı. PR #77.
  - Gerçek uygulamada (geliştirme sürümü, kullanıcının verisi) bakıldı. Klavyede gerçek yüklemeyle denenmedi: K5'in ayrılan hakkı, K8'in dönüşten sonra yenilenmesi, odak zamanlayıcısının kartları (her yükleme flaşa yazar).

### Görünüm ve marka

- **K1. Kurulum dosyasının simgesi NSIS'in hazır simgesi.**
  - Görülen: Explorer'da "NSIS" yazılı küre ve ok. Kurulum ve kaldırma pencerelerinin başlığında da aynı simge var.
  - Yapılacak: `bundle.windows.nsis.installerIcon` ve `uninstallerIcon` Keymotion simgesi olacak (`icons/icon.ico`).
  - Kontrol: Explorer'da, pencere başlıklarında ve "Yüklü uygulamalar"da Keymotion simgesi görünüyor.
- **K2. Kurulum ve kaldırma görselleri NSIS'in hazır görselleri.**
  - Görülen: karşılama ve bitiş sayfasının solunda lacivert, damalı, eski bir bilgisayar resmi (164×314). Kaldırma sayfasının sağ üstünde kırmızı NSIS kaldırma logosu (üst bant, 150×57). Kullanıcı: "Logolar da berbat."
  - Yapılacak: Keymotion'ın kendi görselleri, BMP olarak:
    - `sidebarImage` 164×314;
    - `headerImage` ve `uninstallerHeaderImage` 150×57.
    Uygulamanın koyu zemini, "K" logosu ve keymotion yazısı kullanılır. Yüksek DPI'da bulanıklık denetlenir.
  - Sıra: önce sohbette önizleme ve seçenekler, onaydan sonra dosyalar (hafıza kuralı).
- **K3. Kurulumun karşılama metni NSIS'in genel metni.**
  - Görülen: "Kurulumu başlatmadan önce çalışan diğer programları kapatmanızı öneririz. Böylece bilgisayarınızı yeniden başlatmadan bazı sistem dosyaları sorunsuz kurulabilir." Teknik ve gereksiz.
  - Yapılacak: `customLanguageFiles` ile sade Türkçe ve İngilizce metin (ne kurulacağı, yönetici izni gerekmediği). Diğer sayfaların metinleri de gözden geçirilir.
- **K11. Genel bakış sayfası çok sade.**
  - Görülen: klavye çiziminin altındaki dört kart (Işık, Ekran, Odak zamanlayıcısı, Profil) çok basit; uygulamanın bütün özelliklerini yansıtmıyor.
  - Yapılacak: Genel bakış'ın yeniden tasarımı. Uygulamayı görsel olarak anlatan ve hızlı işlem araçları olan bir ana sayfa olacak; örneğin hatırlatıcılar, takvim, sistem göstergeleri, canlı ekran, son görsel, hızlı ışık seçimi. Açılış animasyonu (6b) ve GitHub ekran görüntüleriyle (7) aynı görsel dil.
  - Sıra: önce önizlemeyle birkaç seçenek, kullanıcı seçer, sonra kod.

### Hatalar

- **K4. "GitHub'da bildir" ve "Öneri gönder" tarayıcı yerine Belgeler klasörünü açıyor.**
  - Görülen: rapor penceresi "Bilgiler panoya kopyalandı. Açılan sayfada…" diyor, ama tarayıcı açılmıyor, Explorer'da Belgeler açılıyor.
  - Sebep (büyük olasılık): adresler `explorer.exe <adres>` ile açılıyor (`notify.rs` `open`). Soru işaretli ve parametreli adresi (`…/issues/new?template=bug.yml&title=…`) Explorer adres olarak tanımıyor ve varsayılan klasörü açıyor. GitHub'ın hazır olmamasıyla ilgisi yok: repo, formlar ve özel güvenlik bildirimi hazır.
  - Not: Aşama 4'teki "GitHub'da bildir tarayıcıda sayfa açtı" denemem doğrulanmamıştı; Chrome'da sekme görünmemişti.
  - Yapılacak:
    - adresleri Windows'un kabuğuyla açmak (`ShellExecuteW`, "open"); `open_link` (destek listesi) de aynı yolu kullansın;
    - kutunun dışında hem formlu hem düz adresle denemek.
- **K5. Hatırlatıcıların ekran kartı gönderilmiyor.**
  - Görülen: üç denemede de ışık dalgası ve ses geldi, ekran kartı gelmedi:
    - "Su molası" önce kartsız oluşturuldu, sonra karta çevrildi;
    - "Dene" düğmesi;
    - baştan kartla oluşturulan "Hareket molası".
  - Olası sebepler (`reminders.rs`): kartlar kapalı, canlı mod kapalı, ekranı toplantı kartı tutuyor ya da yükleme hakkı (bütçe) dolu. Kullanıcı aynı oturumda odak zamanlayıcısıyla art arda kart göndermişti (K8). Kart atlanınca kullanıcıya hiçbir şey söylenmiyor; sebep yalnızca günlükte.
  - Yapılacak:
    - geliştirme sürümünde yeniden üretip sebebi bulmak;
    - atlanan kartın sebebi arayüzde sade dille görünsün (örnek: "Ekran kartı gönderilmedi: kısa sürede çok görsel gönderildi, birazdan yeniden denenecek");
    - "Dene" düğmesinin neyi yapıp yapmadığı açık olsun;
    - sonradan karta çevrilen hatırlatıcı da denensin.
  - **Bulunan sebep ve yapılan (PR #73):**
    - Bekleyen odak kartı her yeni yükleme hakkını gelir gelmez (50 ms içinde) alıyordu; beklemeyen hatırlatıcı kartı her seferinde boş bütçe buluyordu. Artık odak zamanlayıcısının ve şarkının kartları son hakkı hatırlatıcıya ve toplantıya bırakıyor (`live.rs` `keep_for`).
    - Satırdaki "Dene" yalnızca ışığı ve sesi deniyordu; kart, düzenleyicinin 3. adımındaki ayrı düğmedeydi. Artık satırdaki "Dene" hatırlatıcının yaptığını yapıyor: ışık, ses ve kart açıksa kart (elle yükleme gibi sayılır, 1 dakika sonra kendi görsel döner) ve sonucu söylüyor.
    - Atlanan kartın sebebi satırın altında: kartlar kapalı, canlı ekran kapalı, USB kablosu yok, toplantı kartı var ya da günlük sınır dolmuş. Sınır doluyken sayfa ayrıca uyarıyor. Kablo yokken kart artık denenmiyor (önceden sessizce düşüyordu).
    - Sonradan karta çevrilen hatırlatıcı: kayıt doğru (günlükte `card=true`); sorun bütçeydi.
- **K8. Ekran sayfası, klavyede olmayan kartı klavyedeymiş gibi gösteriyor.**
  - Görülen: odak zamanlayıcısında aşamalar art arda değişti, yükleme hakkı doldu. Uygulama "şimdi gönder?" diye sordu; kullanıcı klavyeyi yıpratmamak için göndermedi. Klavyede eski animasyon kaldı, ama Ekran sayfasında uzun molanın kartı güncel görünüyor.
  - Yapılacak:
    - Ekran sayfası ve Genel bakış'taki Ekran kartı klavyede gerçekten olanı göstersin;
    - bekleyen (gönderilmemiş) kart ayrıca "bekliyor" diye görünsün (`live.rs` `pending`);
    - "Gönderme" denince bekleyen kart düşsün ve sayfalar eski görseli göstersin.
  - **Bulunan sebep ve yapılan (PR #73):**
    - Kendi görsele dönüş `last.atMs`'yi değiştirmiyor; arayüz Ekran verisini yalnızca o değişince yeniden okuyordu, bu yüzden son canlı kartı göstermeye devam ediyordu. Artık her yükleme bitince yeniden okuyor.
    - Bekleyen kart Ekran sayfasında ve Genel bakış'ta ayrı bir satırda ("Sırada bekleyen kart: …, henüz klavyeye gönderilmedi").
    - "Şimdi gönder"in yanında "Gönderme" düğmesi: bekleyen kart düşer, klavyede şu anki görsel kalır (`live_drop_waiting`). Odak zamanlayıcısının uyarısındaki "önceki kart ekranda kalır" düzeltildi.
- **K10. Kaldırma penceresinde "Uygulama verilerini sil" kutusu işaretli görünüyor.**
  - Varsayılanı işaretsiz olmalı (Tauri şablonu). Kullanıcı işaretlediyse gerçek AppData'daki deneme verisi silindi; asıl veri Claude'un kutusunda, etkilenmedi.
  - Yapılacak: kutunun dışında varsayılan durumu doğrulamak.

### Davranış ve tasarım kararları

- **K6. Odak zamanlayıcısında kısa mola görünmedi.**
  - Görülen: odak, kısa mola, uzun mola 1 dk ve tur 1; "Atla" ile ilerlenince odak → uzun mola → bitti. Kısa mola hiç gelmedi.
  - Sebep: tasarım böyle (`pomodoro.rs`): son odaktan sonra uzun mola, aradaki odaklardan sonra kısa mola gelir. Tur 1 olunca kısa mola yok. Hata değil, ama kullanıcıya anlaşılmaz geliyor; K7 bunu çözer.
- **K7. Odak zamanlayıcısının yeniden tasarımı (kullanıcının isteği).**
  - Kullanıcı: "Kısa mola ile uzun mola arasındaki fark yalnızca süre; saçma."
  - Yapılacak: sabit "odak, kısa mola, uzun mola, tur" yerine kullanıcının kurduğu bir sıra:
    - bloklar: odak ya da mola, süre, isteğe bağlı ad;
    - "Mola ekle" ve "Odak ekle";
    - bloklar fareyle sürüklenip yer değiştirir;
    - tekrar sayısı.
  - Ekran kartları, ışıklar, tepsi menüsündeki aşamalar ve "Atla" bu sıraya göre çalışır. Eski ayarlar yeni yapıya taşınır (veri taşıma, ders 54).
  - Sıra: önce önizlemeyle tasarım, sonra kod.
- **K9. Pencere kapatılınca bildirim gelmedi.**
  - Sebep: "arka planda çalışıyor" bildirimi bilgisayar başına bir kez gösterilecek şekilde yapıldı (madde 39; kullanıcının "durmadan bildirim göndermeyelim" isteği). Gerçek AppData'da bu hak 06:04'te `target\release\keymotion.exe` açılışında kullanıldı (logosuz çıkan bildirim). Kurulu sürüm aynı veriyi kullandığı için bir daha göstermedi.
  - **Karar (kullanıcı, 2026-11-12): her açılışta bir kez.** Uygulama her açıldığında, pencere ilk kapatılınca bildirim bir kez gelir; aynı oturumda sonraki kapatmalarda gelmez.
  - Yapılacak: `tray.rs` `take_notice` kalıcı `notice_shown` yerine oturum başına bir bayrak kullanır. `background.json` içindeki eski alan okunur ama artık kullanılmaz. Windows açılınca gizli başlamada (`--hidden`) pencere hiç gösterilmediği için bildirim gelmez.
  - Logo hâlâ doğrulanmadı; bildirim çıkınca kutunun dışında bakılacak. Hatırlatıcının Windows bildirimi de kurulu sürümde denenecek.

## Aşama 5d: kullanıcının geri bildirimleri ve denemeleri (2026-11-12, öğleden sonra)

Kullanıcı 5c'nin sonucunu gördü. **Sıra (kullanıcının talimatı):**
1. Belgeler güncellenir; kullanıcı oturumu sıkıştırır.
2. Sıkıştırmadan sonra önce aşağıdaki geri bildirimler (K12, K13) düzeltilir.
3. Sonra kullanıcı, bu bölümdeki deneme listesini dener; bulduğu sorunlar düzeltilir.
4. Hepsi bitince yol haritasına dönülür: Aşama 6, sürüm hattı.

### Geri bildirimler (sıkıştırmadan sonra ilk iş)

- **K12. Genel bakış'taki klavye çizimi kötü görünüyor.**
  - Kullanıcı: Genel bakış "biraz daha düzgün olmuş, güzel", ama klavye çizimi görsel olarak çok kötü; eskiden tuşların üstünde harfler ve rakamlar yazıyordu.
  - Sebep: yeni düzende (PR #77) çizim, ekran görselinin yanında daha dar bir sütunda (1.6fr / 1fr) duruyor. `KeyboardView` tuş yazılarını çizimin genişliğine göre gösteriyor (`useLegendSize`): eşiğin altında yazılar tamamen gizleniyor.
  - Yapılacak: çizim tuş yazılarıyla, düzgün ve okunur olsun. Seçenekler:
    - klavye üstte tam genişlikte, ekran görseli altta ya da yanda daha küçük;
    - yazı eşikleri ve boyutları bu genişliğe göre;
    - ikisi birlikte.
  - Önce önizlemeyle bir ya da iki seçenek gösterilir (tasarım kuralı), kullanıcı seçer, sonra kod. Ekran ve sistem genişlikleri (geniş ve dar pencere) denenir.
  - **Yapıldı (2026-11-12): kullanıcı A'yı seçti** (önizlemede A, klavye üstte tam genişlikte; B, yan yana ve klavye büyük).
    - Klavye çizimi sayfanın tam genişliğinde; altında "Klavye ekranında şimdi" ilk kutu, iki satır yüksekliğinde. Geniş pencerede sistem göstergeleri iki sütun kaplıyor, son satır dolu.
    - Ölçümler: geniş pencerede (1320) çizim 948 px, tuş yazıları 13 px; dar pencerede (1100) 928 px, 13 px; en küçük pencerede (960) 788 px, 11,5 px. Dar pencerede kenar çubuğu daraldığı için çizim geniş kalıyor. Gerçek uygulamada (%125) doğrulandı.
- **K13. Açılış animasyonundaki logo pikselli, düşük çözünürlüklü görünüyor.**
  - Kullanıcı: animasyonun üstündeki logo "piksel piksel" geliyor; Claude'un aldığı ekran görüntüsünde de görülüyor. Altındaki "keymotion" yazısı düzgün.
  - Yapılacak: logo her ölçekte keskin olsun. İncelenecek olası sebepler:
    - `.intro-stage`'deki `will-change: transform`: katman bir kez bitmap olarak çiziliyor;
    - SVG'nin içindeki `rect`'in `transform: scale` animasyonu;
    - `pathLength="1"` ile kesik çizgi (`stroke-dasharray`) çizimi;
    - pencerenin ölçeği (Windows %125/%150) ile yakın plan karşılaştırma: gerçek WebView2'de, cihaz ölçeğinde çekilen kareyle.
  - Bitti sayılır: gerçek pencerede, cihaz ölçeğinde çekilen karelerde logonun kenarları yazı kadar keskin; kullanıcı görüp onaylar.
  - **Kapandı (2026-11-12), kod değişikliği gerekmedi.** Animasyon kare kare yakalandı: uygulamanın kendi çiziminden (CDP, sayfa yenilenerek ve soğuk açılışta) ve ekrandan (pencerenin bölgesi, %125). Her karede logo yazı kadar keskindi; ekran ölçeği açılıştan itibaren 1,25'te sabit. Kullanıcı denemeler sırasında açılan animasyonu gördü: "artık daha keskin, daha net" ve onayladı.
    - Önceki pikselli görüntü yeniden üretilemedi. Olası sebep: güncellemeden sonraki ilk açılışta geliştirme sunucusunun dosyaları hazırlaması (çizim takılıyor). Yeniden görülürse hangi açılışta olduğu not edilir (deneme 7).
    - Ders: ekrandan görüntü yakalarken pencere çizilmeden önceki kareler pencerenin arkasındakini alabilir; yakalama pencere çizildikten sonra başlatılır, böyle kareler hemen silinir.

### Kullanıcının denemesi (K12 ve K13'ten sonra)

Claude'un denemediği ya da yalnızca tarayıcı önizlemesinde denediği her şey. Klavyeye yükleme yapan adımlar klavyenin belleğine yazar; bunlar kullanıcının kendi eylemi.

**A. Geliştirme sürümünde (uygulama açıkken):**
1. **K4, GitHub bağlantıları:** Ayarlar › Geri bildirim › "Sorun bildir" penceresinde "GitHub'da bildir", ayrıca "Öneri gönder": tarayıcıda GitHub'ın formu açılmalı, Belgeler klasörü açılmamalı.
2. **K9, arka plan bildirimi:** pencereyi kapat: "Arka planda çalışıyor" bildirimi gelmeli. Tepsiden aç ve yine kapat: gelmemeli. Uygulamadan çık, yeniden aç, kapat: yine bir kez gelmeli.
3. **K7, odak zamanlayıcısı:**
   - "25 / 5 klasik"i seç (şu anki ayar kurulum denemesinden kalma "1 dk odak, 1 dk mola");
   - blok ekle, sil, tutamaçtan sürükle, ok tuşlarıyla taşı; "Sıra bitince" seçenekleri;
   - Başlat: tuş ışıkları, tepsi menüsünde "Odak · Blok 1/6 · … dk kaldı", kenar çubuğundaki satır; "Atla" ile bloklar arasında geçiş;
   - ekran kartları ve canlı ekran açıksa kartlar klavyeye yüklenir: "1/3 odak", "Sonra: 5 dk mola".
4. **K8, Ekran sayfası:** odak zamanlayıcısıyla art arda kart gidip yükleme hakkı dolunca:
   - Ekran sayfasında ve Genel bakış'ta "Sırada bekleyen kart: …, henüz klavyeye gönderilmedi" satırı;
   - "Şimdi gönder"in yanındaki "Gönderme": bekleyen kart düşmeli, klavyede şu anki görsel kalmalı;
   - seri bitip kendi görseline dönünce sayfalar kendi görselini göstermeli (önceden son kartı gösteriyordu).
5. **K5, hatırlatıcı kartı:**
   - kartlı bir hatırlatıcının satırındaki "Dene": canlı ekran açıkken ışık, ses ve kart, altında "Kart gönderildi, 1 dakika ekranda kalır"; canlı ekran kapalıyken "Yalnızca ışık ve ses denendi: canlı ekran kapalı";
   - hatırlatıcı kendi saatinde çalınca kart gelmeli; gelmezse satırın altında sebebi yazmalı;
   - yükleme hakkı doluyken sayfanın üstünde uyarı; odak zamanlayıcısı çalışırken bile hatırlatıcının kartı gidebilmeli (son hak hatırlatıcıya ayrılıyor).
6. **K11 ve K12, Genel bakış:** kutular gerçek verilerle; klavye çizimi tuş yazılarıyla; odak kutusundan "Başlat".
7. **6b ve K13, açılış animasyonu:** uygulamadan çık ve yeniden aç: K çizgi çizgi çizilmeli, logo keskin olmalı, logo kenar çubuğuna gitmeli. Tepsiden açınca oynamamalı. Tıklayınca ya da bir tuşa basınca hemen bitmeli. Logo yeniden pikselli görünürse hangi açılışta olduğunu not et (güncellemeden sonraki ilk açılış mı, Windows'la açılış mı, sonraki açılışlar mı).

**B. Sondaki kurulum derlemesiyle (Explorer'dan, Claude'un kutusunun dışında; ders 62):**
8. **K1, K2, K3, kurulum:** kurulum dosyasının ve pencerelerin simgesi Keymotion; karşılama ve bitiş sayfasının solunda Keymotion görseli, üst bantta logo; karşılama metni sade ("Keymotion, klavyenizin ışıklarını…"); diğer sayfaların Türkçesi.
9. **K10, kaldırma:** "Ayarlarımı ve kayıtlarımı da sil" kutusu işaretsiz gelmeli; kaldırma penceresinin üst bandında logo.
10. **K9, arka plan bildirimi ve logosu:** "Arka planda çalışıyor" bildiriminde Keymotion adı ve logosu. Ayrıca A'daki 2. madde burada denenir: pencereyi kapat, bildirim gelir; tepsiden açıp yine kapat, gelmez; uygulamadan çıkıp yeniden aç ve kapat, yine bir kez gelir.
11. **6b, açılış süresi:** derlenmiş sürümde animasyon açılışı uzatmamalı (ölçülür).
12. **K4, kurulu sürümde:** GitHub bağlantıları yine tarayıcıda açılmalı.

### Denemelerin sonuçları

**A, geliştirme sürümünde (2026-11-12, öğleden sonra):** 1, 3, 4, 5, 6 ve 7 çalıştı. Bulunanlar:
- **2 (K9), arka plan bildirimi hiç gelmedi.** Sebep ortam: geliştirme sürümünü Claude başlattığı için Claude'un kutusunda çalışıyor. Uygulama bildirimlerde adı ve logosu görünsün diye kayıt defterine (`HKCU\Software\Classes\AppUserModelId\dev.keymotion`) yazıyor; bu kayıt yalnızca kutunun içinde var, gerçek Windows'ta yok (kutunun dışından bakıldı). Windows tanımadığı uygulamanın bildirimini sessizce atıyor; uygulamanın kaydında hata yok, "window hidden" satırı var. Kod değişmedi; deneme B'nin 10. maddesine taşındı (kurulu sürüm kaydı gerçek Windows'a yazar).
- **K14. Bir hatırlatıcının satırındaki "Dene": kart ve ışıklar geldi, ama bir dakika sonra kendi görseli geri gelmedi.** Sebep: "Dene" kartı hemen gönderir ve bir dakika sonrası için dönüş kurar; dönüş, sırada gönderilmeye hazır bir kart görünce "ekrana o gelecek" diye iptal ediliyordu, oysa o kart hatırlatıcının kendi kartıydı. Takvimdeki "Dene" de aynıydı; hatırlatıcının ve toplantının kendi saatinde gelen kartı etkilenmiyordu (dönüş, kart ekrandan kalkarken kuruluyor). **Düzeltildi:** dönüşü yalnızca başka bir kaynağın hazır kartı iptal ediyor; kaynağın kendi kartı gönderilirken dönüş onu bekliyor ve süresi dolunca kendi görseline dönülüyor (`live.rs` `back_inputs`, birim testi). Kullanıcı "Dene"yi yeniden dener.

## Aşama 6: sürüm hattı ve otomatik güncelleyici

- **GitHub Actions:**
  - `v*` etiketi geldiğinde Windows'ta derler. Sürüm dosyası yalnızca burada üretilir, hiçbir zaman bilgisayardan yüklenmez: SignPath "kaynaktan doğrulanabilir, otomatik derleme" şartı koyuyor (Aşama 9);
  - imza adımı için yer bırakılır; Aşama 9'da SignPath onaylayınca eklenir;
  - kurulum dosyasının SHA-256 özeti sürüm sayfasına yazılır;
  - kurulum dosyasını ve `latest.json`'u GitHub Releases'e taslak olarak yükler;
  - sürüm notlarını `CHANGELOG.md`'den alır.
- **Güncelleyici:** `tauri-plugin-updater`.
  - İmza anahtarı `tauri signer generate` ile üretilir. Özel anahtar GitHub Secrets'a konur; bunu kullanıcı onayıyla yap. Ortak anahtar `tauri.conf.json`'a girer.
  - Uygulamada Ayarlar'da "Güncellemeleri denetle" bulunur.
  - Başlangıçta günde bir kez sessiz kontrol yapılır. Ağa yalnızca GitHub için çıkılır.
  - Gizlilik belgeleri güncellenir: uygulama ağa takvimler ve güncelleme denetimi için çıkar.
- **Dallar:** sürüm için `develop` → `main` PR'ı açılır, kullanıcı onayıyla birleşir. Etiket `main`'de atılır.
- **Deneme:** 0.1.0-rc → 0.1.0-rc.2 güncellemesi gerçekten iniyor ve kuruluyor mu?
- **Kullanıcının kararları (2026-11-12):** güncelleyicinin tasarımı önizlemeden onaylandı (Ayarlar › Hakkında'da satır ve "Güncellemeleri kendiliğinden denetle", yeni sürümde sayfanın üstünde şerit, kurulum penceresi); plan "Uygun, başla".
- **Yapılan (2026-11-12):**
  - Güncelleyici (`updates.rs`, `tauri-plugin-updater`): açılıştan 30 sn sonra ve sonra saatte bir "bir gün geçti mi" bakılır, geçtiyse GitHub'a sorulur (`updates.json`: anahtar ve son denetim). Kurulum, klavyeye yazma bitene kadar bekler (en çok bir ekran yüklemesi süresi); Windows'ta uygulama kapanır, kurulum küçük bir pencerede ilerler (`installMode: passive`). İndirme durdurulabilir. Hatalar sade dille: internet yok, henüz sürüm yok, imza tutmadı, klavyeye yazma bitmedi.
  - Deneme sürümleri (sürümde "-" eki) GitHub'ın sürüm listesinden en yeni yayımlanmış `latest.json`'u okur, ön sürümler dahil; normal sürümler yalnızca `releases/latest`'e bakar. Sebep: derlenmiş sürüm güncellemeyi yalnızca https'ten indiriyor, yerel bir sunucuyla deneme yapılamıyor; rc.1 → rc.2 denemesi GitHub'daki ön sürümlerle yapılacak.
  - Sürüm notları `latest.json`'da `{"en", "tr"}` olarak durur; uygulama arayüz dilindekini gösterir, o dilde not yoksa GitHub'daki sürüm sayfasına bağlantı verir (Türkçe arayüzde İngilizce metin gösterilmez).
  - İmza anahtarı üretildi: gizli yarısı ve parolası proje sahibinin kullanıcı klasöründe, `.tauri\keymotion\` altında (depoda değil); ortak yarısı `tauri.conf.json`'da.
  - Sürüm hattı (`.github/workflows/release.yml`, `tools/release/latest-json.mjs`, testleri CI'da): etiket ya da elle başlatma, sürüm ve not denetimi, derleme, SHA-256, `latest.json`, taslak sürüm; SignPath için yer ayrıldı (imza sonrası `.sig` yeniden üretilmeli).
  - Gizlilik belgesi ve Ayarlar'daki gizlilik metni güncellendi; `docs/gelistirme-akisi.md`'ye sürüm çıkarma adımları eklendi.
  - Denendi: tarayıcı önizlemesinde (iki dilde; yeni sürüm, indirme, vazgeçme, internet yok, henüz sürüm yok) ve gerçek uygulamada (günlük denetim ve düğme GitHub'a gitti, henüz sürüm olmadığı için "GitHub'da henüz yayımlanmış bir sürüm yok").
- **Kalan (kullanıcının onayıyla):**
  1. **Yapıldı (2026-11-12, kullanıcının onayıyla):** gizli anahtar ve parolası GitHub Secrets'ta (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). Yedeği kullanıcı kendi parola yöneticisine alır.
  2. İş akışını `develop`'dan elle bir kez çalıştırmak (derlenmiş sürüm, taslak; incelenip silinir).
  3. Uçtan uca deneme: 0.1.0-rc.1 ve 0.1.0-rc.2 ön sürüm olarak yayımlanır (herkese açık, ama "en son sürüm" sayılmaz), rc.1 kurulur, rc.2'ye güncellenir. Sonra ön sürümler silinebilir.

## Aşama 6b: uygulama penceresinin açılış animasyonu

Kullanıcının kararı (2026-11-12): uygulama açılırken bir giriş animasyonu olacak. Bugün yok; pencere hazır olunca doğrudan açılıyor. Klavyenin ekranındaki "Keymotion animasyonu" (Aşama 3) ve klavye yokken çıkan hoş geldiniz ekranı bu değil; ikisi de pencerenin açılışında görünmez.

- **Ne zaman:** Aşama 7'den önce. Ekran görüntüleri ve GitHub sayfasındaki tanıtım, uygulamanın son görünümüyle hazırlanır.
- **Araştırılacak (nasıl yapılır):**
  - Tauri 2'de açılış ekranı yolları: ayrı küçük bir açılış penceresi mi, yoksa ana pencerede arayüz yüklenene kadar duran bir katman mı. Bugün pencere görünmez açılıyor (`visible: false`) ve arayüz hazır olunca gösteriliyor; saydam ve Windows 11'in yarı saydam arka planı (Mica) ile uyumu.
  - Ölçüm: kurulu sürümde pencerenin görünmesi ve arayüzün hazır olması kaç ms sürüyor (derlenmiş sürümle; ders 61). Animasyon açılışı geciktirmemeli; uygulama hazır olunca bitmeli ya da kısalmalı.
  - Ne zaman oynamaz:
    - Windows açılınca gizli başlarken (`--hidden`);
    - pencere tepsiden yeniden açılırken; yalnızca uygulamanın gerçek açılışında oynar;
    - Windows'un "animasyon efektleri" kapalıysa sade bir geçiş (`MotionConfig reducedMotion`).
  - Başka masaüstü uygulamalarının açılış ekranları örnek olarak incelenir, kopyalanmaz.
- **Hazırlanacak (nasıl görünür):**
  - Klavyedeki animasyonla aynı dil: logodaki "K" çizgi çizgi çizilir, turuncu çizgi parlar, "keymotion" yazısı açılır, uygulamaya yumuşak geçiş. Kısa: yaklaşık 1 sn ya da daha az.
  - Açık ve koyu temada, uygulamanın renkleri ve Bricolage yazı tipiyle.
  - Önce sohbette etkileşimli önizlemeyle iki ya da üç seçenek gösterilir; kullanıcı seçer, sonra kod yazılır (hafıza kuralı).
- **Bitti sayılır:** kurulu sürümde açılış ölçülmüş, animasyon açılışı uzatmıyor, gizli başlamada ve tepsiden açmada oynamıyor, kullanıcı görüp onaylamış.
- **Yapılan (2026-11-12, PR #78; kullanıcı A'yı seçti):**
  - Araştırmanın sonucu: ayrı bir açılış penceresi yerine ana pencerede bir katman. Tauri pencereyi arayüz yüklenmeden gösteriyor (`lib.rs` `setup`), bu yüzden animasyon `app/index.html`'in içinde: pencerenin ilk karesinden oynar, ikinci bir WebView açmaz.
  - Sıra (yaklaşık 0,95 sn): logo kutusu belirir, K'nin dikey çizgisi, üst çaprazı, turuncu çizgisi sırayla çizilir, turuncu çizgi parlar, "keymotion" harf harf açılır.
  - Bitiş (`src/lib/intro.ts`): ilk sayfa hazır olunca (en geç 4 sn) animasyonun kendisi bitene kadar beklenir (tıklama ya da tuş hemen bitirir), sonra logo kenar çubuğundaki logoya küçülerek gider ve katman 280 ms'de solar.
  - Gizli başlamada (`--hidden`) katman hemen kaldırılır; tepsiden açılışta zaten yoktur. Windows'un animasyon efektleri kapalıysa logo ve yazı hareketsiz görünür ve solar.
  - Tema ilk kareden önce uygulanır (`public/theme-boot.js`; içerik güvenlik kuralı satır içi betiğe izin vermiyor).
  - Geliştirme sürümünde: normal açılışta katman 2,3 sn'de kalktı (geliştirme sürümü yavaş yükleniyor), gizli başlamada 0,9 sn'de. Kalan: derlenmiş sürümde ölçüm ve kullanıcının görmesi (sondaki kurulum derlemesiyle).

## Aşama 7: GitHub sayfası ve bütün belgeler

Kullanıcının kararı (2026-11-12): GitHub sayfası yalnızca imza şartları için yapılmaz. Bizim belgelerle ilgili bütün notlarımız ve isteklerimiz, SignPath'in şartları ve ürünün görünümü bu aşamada bir araya gelir. GitHub sayfası baştan sona, tek bir yapıyla düzenlenir; ürünün GitHub'daki profesyonel görünümü ve içeriği burada tamamlanır.

Bu aşamada birleşen kaynaklar:
- `docs/urun-eksikleri.md` bölüm H (kullanıcının notu, 2026-11-11): iki dilli sayfalar, ekran görüntüleri, kullanım belgeleri, topluluk dosyaları;
- `docs/DURUM.md` §7 P3 "İlk sürümde belge ve GitHub gözden geçirmesi";
- hafızadaki karar: belgeler geliştirme bitince, ürünün son haline göre topluca düzenlenir;
- SignPath Foundation'ın sayfa şartları (Aşama 9).

### 7.1 Önce yapı ve görünüm (onaydan sonra yazım)

- **Örnekler:** büyük açık kaynak masaüstü uygulamalarının repoları ve belge siteleri incelenir; iyi örnekler kullanıcıya gösterilir. Kopyalanmaz, örnek alınır.
- **İçindekiler:** GitHub sayfasının bölümleri ve sayfa listesi önce çıkarılır, kullanıcıyla kararlaştırılır.
- **Biçim:** belgeler GitHub'da Markdown sayfaları olarak mı kalacak, yoksa ayrıca bir belge sitesi mi (GitHub Pages) olacak; aşamanın başında karar verilir. Görünüm önce sohbette etkileşimli önizlemeyle gösterilir (hafıza kuralı).
- **Dil düzeni:** İngilizce varsayılan, Türkçe tam karşılığı. README için `README.md` ve `README.tr.md`; ikisinin en üstünde "English | Türkçe" bağlantısı. Diğer sayfalarda da aynı düzen.

### 7.2 İçerik (bölümler, taslak)

- **Tanıtım:** ne yapar, ekran görüntüleri, "son sürümü indir" bağlantısı, desteklenen klavye ve denenen ortam (Windows 11 26200, klavye yazılımı 1.06, WebView2 sürümü).
- **Kurulum ve ilk açılış:** indirme, imzasız sürümde Windows uyarısını geçme ("Ek bilgi" → "Yine de çalıştır"), klavyeyi bağlama, ilk açılış.
- **Kullanım:** sayfa sayfa anlatım, iki dilde, ekran görüntüleriyle: Genel bakış, Işıklar, Tuş atama, Makrolar, Ekran, Profiller, Bağlantı, Odak zamanlayıcısı, Sistem göstergeleri, Hatırlatıcılar, Takvim, Ayarlar.
- **Özellikler ve bağlantı modları tablosu:** `docs/destek-haritasi.md`'den; İngilizceye çevrilir.
- **Kurtarma rehberi (B9):** fabrika animasyonunu geri yükleme, tuş atamalarını sıfırlama, "bir şey ters giderse".
- **Sık sorulanlar.**
- **Gizlilik:** `PRIVACY.md` ve `docs/gizlilik.md`; SignPath'in istediği cümle: "This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it." (takvimler ve güncelleme denetimi kullanıcının isteğiyle).
- **Güvenlik:** `SECURITY.md` (Aşama 4'te yapıldı).
- **Kod imzalama politikası (Code signing policy):** SignPath şartı. Ana sayfada ve indirme/sürüm sayfalarında bu adla bir bölüm ya da bağlantı. İçinde: "Free code signing provided by SignPath.io, certificate by SignPath Foundation", ekip rolleri (yazarlar, inceleyenler, onaylayanlar) ve kişiler. Sayfa bu aşamada hazırlanır; SignPath cümlesi onay gelince (Aşama 9) eklenir.
- **Katkı:** `CONTRIBUTING.md` (iki dilde; PR'lar `develop`'a, denetimler, donanım testi kuralları), davranış kuralları, PR şablonu, etiketler (`bug`, `enhancement`, `keyboard support`, `needs-hardware-test`, `model:…`), sürüm kilometre taşları.
- **Lisans ve haklar:** LICENSE-MIT ve LICENSE-APACHE, NOTICE (AJAZZ'dan bağımsızlık notu), üçüncü taraf lisansları.
- **Topluluk:** Discussions ve sorun formlarına bağlantılar.
- **Sürüm sayfası:** sürüm notu (CHANGELOG'dan), kurulum dosyası ve SHA-256, iki dilde kısa özet, "Code signing policy" bağlantısı.

### 7.3 Ekran görüntüleri

- TR/EN × açık/koyu. Arayüz kesinleştikten sonra çekilir. Sahte cihazla çekilebilir.

### 7.3b Desteklenen klavyelerin gerçek ürün görselleri (kullanıcının fikri, 2026-11-12)

- **Amaç:** tanıtımda desteklenen klavyelerin gerçek fotoğrafları da yer alır. Ziyaretçi uygulamanın hangi klavyelerde çalıştığını ilk bakışta anlar.
- **Nerede:** README'nin üst kısmı ve "Desteklenen klavyeler" bölümü; ileride her model için bir satır (fotoğraf, model adı, bağlantı modları, desteklenen özellikler).
- **Görsellerin kaynağı (haklar):**
  - tercih: kullanıcının kendi klavyesinin kendi çektiği fotoğrafları (gerekirse uygulamadaki ışık ve ekran görselleriyle birlikte, ürünü çalışırken gösterir);
  - AJAZZ'ın resmî ürün fotoğrafları telif hakkı onlara ait olduğu için ancak izinle kullanılır; fabrika animasyonundaki kararla aynı ilke (\`urun-eksikleri.md\` kararlar 3);
  - model adları yalnızca uyumluluğu söylemek için kullanılır ("AJAZZ AK980 PRO ile çalışır"); "resmî değildir" notu görsellerin yanında da durur.
- **Yeni model eklenince:** destek haritasındaki satırla birlikte o modelin görseli de eklenir.

### 7.4 Bütün `docs/` belgelerinin gözden geçirilmesi

- Geliştirici belgeleri (`DURUM.md`, `gelistirme-akisi.md`, `mimari.md`, `donanim-dogrulama.md`, `teknik-analiz.md`, araştırmalar) güncelliği için okunur. Kullanıcıya yönelik sayfalarla geliştirici belgeleri ayrı yerlerde durur.
- `gelistirme-akisi.md` işleyişi yeniden değerlendirilir (DURUM P3).

### 7.5 SignPath şartlarının denetim listesi (Aşama 9'dan önce hepsi ✓ olmalı)

- [ ] Proje adı aranınca bulunuyor: GitHub sayfası açıklaması, konular (topics) ve ana sayfa adresi dolu.
- [ ] İndirme sayfası programın ne yaptığını anlatıyor ("Documented").
- [ ] Ana sayfada ve sürüm sayfasında "Code signing policy" bölümü ya da bağlantısı.
- [ ] Gizlilik cümlesi ya da gizlilik politikası bağlantısı.
- [ ] Ekip rolleri yazılı.

## Aşama 8: son denemeler ve ilk sürüm (0.1.0)

- **Kullanıcının elle denemeleri** (ben yapamıyorum):
  - klavye yokken açılış ekranı (kablo çekilir);
  - yükleme ortasında kablo çekip yeniden açınca "yarım kaldı" önerisi;
  - yükleme sırasında Windows'u kapatmak;
  - küçük ekranda (1920×1080, %150) pencerenin sığması;
  - Bluetooth kanal kısayolları (Fn+Q/W/E) ve denenmemiş tuş atamaları (B8);
  - hatırlatıcı Windows bildiriminin ekranda göründüğü.
- **Kurulu sürümle uçtan uca deneme:** temiz kurulum, güncelleme akışı, kaldırma.
- **Yayın:** `CHANGELOG.md` ve sürüm notları. `main`'e birleştirme kullanıcı onayıyla. Sonra etiket ve yayın.
- **İmzasız ilk sürüm:** sürüm notu Windows uyarısını ve "Ek bilgi" → "Yine de çalıştır" yolunu açıkça anlatır; imzanın ileride geleceği yazılır.
- **Kullanım kanıtı (Aşama 9 için):** sürümden sonra duyuru. Nerede paylaşılacağına (klavye toplulukları, forumlar) kullanıcı karar verir. İndirme sayısı, yıldızlar ve tartışmalar SignPath formundaki "Reputation" alanını doldurur.
- **İsteğe bağlı:** sahte cihazla Playwright uçtan uca testlerini CI'a eklemek (DURUM P3).

## Aşama 9: kod imzalama (SignPath Foundation), ilk sürümden sonra

Eskiden "Aşama 1b: erken başvuru" idi. 2026-11-12'de SignPath'in şartları ve başvuru formu okununca ilk sürümden sonraya alındı (kullanıcının kararı).

- **Neden imza:** imzasız bir kurulum dosyasını Windows SmartScreen "Windows bilgisayarınızı korudu", "Yayımcı: Bilinmiyor" ekranıyla durdurur; indirenler korkar. İmza "Bilinmiyor" yazısını kaldırır ve itibar biriktirir; yeni bir yayımcıda ilk zamanlarda uyarı yine çıkabilir, indirme arttıkça kalkar. İmzasız dosyada itibar hiç birikmez.
- **Seçenek A (öneri): SignPath Foundation.** Açık kaynak projelere ücretsiz.
  - **Yayımcı:** sertifika SignPath Foundation adına çıkar; Windows yayımcı olarak "SignPath Foundation" gösterir, kullanıcının adını değil.
  - **Başvuru formunun alanları** (signpath.org/apply, 2026-11-12'de okundu):
    - zorunlu: proje adı ("Google'da aranınca projeyi açıkça bulmalı"), repo adresi, ana sayfa adresi (repo sayfası olabilir), kısa tanıtım cümlesi, kategori, açıklama, **itibar** (projenin kullanıldığını gösteren bağlantılar: haber, blog, indirme istatistiği, GitHub istatistikleri, topluluk tartışmaları), otomatik derleme sistemi, ad, soyad, e-posta, SignPath'i nereden duyduğu;
    - isteğe bağlı: indirme sayfası (SignPath kullandığını yazmalı), gizlilik politikası (veri toplayan programlar için zorunlu), Wikipedia sayfası, yöneten kuruluş türü, şirket.
  - **Şartlar** (signpath.org/terms):
    - OSI onaylı lisans, "ticari çift lisans olmadan". Bizim "MIT OR Apache-2.0" ticari değil, ikisi de açık kaynak; büyük olasılıkla uygun, başvururken teyit edilecek;
    - etkin olarak geliştiriliyor; **imzalanacak biçimde zaten yayınlanmış**; indirme sayfasında işlevi anlatılmış;
    - SignPath başvuruda projenin itibarını ve kontrolünü denetler; programlar için "doğrulanabilir bir itibar" ister. Kabul onların takdirinde;
    - ekipteki herkes GitHub'da ve SignPath'te iki aşamalı doğrulama kullanır;
    - sürüm dosyası kaynaktan doğrulanabilir biçimde, otomatik derlenir (Aşama 6); her imza için elle onay;
    - ana sayfada "Code signing policy" (Aşama 7.2);
    - kaldırma imkânı, sistem değişikliklerinin duyurulması, ürün adı ve sürüm tutarlılığı (Aşama 5);
    - SignPath Foundation imzalanan yazılımdan sorumluluk kabul etmez; ücretsiz abonelik bildirimsiz durdurulabilir.
  - **Onay süresi:** hiçbir yerde yazmıyor.
- **Başvurudan önce** (hepsi ✓ olmalı):
  - [ ] Aşama 8: imzasız 0.1.0 GitHub Releases'te;
  - [ ] Aşama 7.5'teki denetim listesi;
  - [ ] Aşama 6: sürüm GitHub Actions'ta derleniyor;
  - [ ] kullanım kanıtı: indirmeler, yıldızlar, bir duyuru ya da tartışma;
  - [ ] kullanıcının GitHub hesabında iki aşamalı doğrulama açık (kullanıcı bakacak; `gh` bunu gösteremedi).
- **Başvuru:** kullanıcı yapar, formu birlikte doldururuz.
- **Onaydan sonra:** sürüm hattına SignPath imza adımı (GitHub Actions entegrasyonu) ve her sürüm için elle onay; "Code signing policy" sayfasına SignPath cümlesi; bir sonraki sürüm imzalı çıkar.
- **Seçenek B: Microsoft'un imza hizmeti (Azure Artifact Signing).** Ücretli (aylık), kişisel kimlik doğrulaması ister, yayımcı olarak kişinin adını gösterir. Bireysel geliştiricilere yalnızca bazı ülkelerde açık olabilir; Türkiye'nin durumu gerekirse kontrol edilecek. SignPath reddederse ya da çok uzarsa ele alınır.

## Sonraya bırakılanlar (ilk sürümden sonra)

- oyun oturumu, ses spektrumu;
- başka klavye modelleri (P4; `docs/destek-haritasi.md`'deki adaylar, ancak o klavyede denenerek);
- 2.4G alıcısı: sürücü ve pil yüzdesi (`20 01`); Bluetooth işleri;
- uygulamaya göre otomatik profil (ön plandaki pencere), tepsi altyapısının üstüne.
- dışarıdan katkı gelmeye başlayınca denetim düzeni: zorunlu inceleme ve CODEOWNERS, etiketler, fork PR'larında gizli bilgi koruması (DURUM P3).

## Bekleyen kullanıcı kararları (sırası gelince sorulacak)

| Aşama | Soru | Öneri |
|---|---|---|
| 1 | Rust paket ve exe adı `keymotion` olsun mu? | Evet |
| 9 | SignPath Foundation başvurusu (ilk sürümden sonra) | Kullanıcı başvurur, formu birlikte dolduralım |
| 5c | K9: arka plan bildirimi ne sıklıkla? | **Karar verildi:** her açılışta bir kez (2026-11-12) |
| 5c | K2 kurulum görselleri, K7 odak zamanlayıcısı sırası, K11 Genel bakış tasarımı | **Karar verildi:** K2 A (sade), K7 önizlemedeki sıra, "25 / 5 klasik" varsayılan, K11 A (kutulu kontrol paneli) (2026-11-12) |
| 6b | Uygulama açılış animasyonunun tasarımı | **Karar verildi:** A, çizgi çizgi K (2026-11-12) |
| 5d | K12: Genel bakış'ta klavye çiziminin yeri ve tuş yazıları | **Karar verildi:** A, klavye üstte tam genişlikte (2026-11-12) |
| 7 | GitHub sayfasının yapısı, biçimi (yalnız repo sayfaları mı, ayrıca belge sitesi mi) ve görünümü | Aşamanın başında önizlemeyle |
| 8 | İlk sürümün nerede duyurulacağı (kullanım kanıtı için) | Kullanıcı seçer |
| 4 | GitHub'da özel güvenlik bildirimi açılsın mı? | Evet (açıldı, 2026-11-12) |
| 5 | Kurulum ayrıntıları (kısayollar, kaldırırken ayarlar) | Yukarıdaki varsayılanlar |
| 6 | Güncelleme anahtarı GitHub Secrets'a konulsun mu? | Evet |
| — | Disk için `cargo clean` (ortak `target` klasörü) | Gerekirse, sorarak |
