# Fikir: Ajanlar için klavye aracı

**Durum:** Açık fikir, şimdi yapılmayacak. Uygulamanın bütün özellikleri (P3 ürünleştirme dahil) bitince ele alınacak. `DURUM.md` §7, "Sonraya bırakılan fikirler".

**Kaynak:** Kullanıcının fikri ve istekleri, 2026-11-10. §2'deki öneri Claude'un aynı gün yazdığı öneri; karar değil. Uygulamaya geçmeden önce kullanıcıyla yeniden konuşulacak.

## 1. Kullanıcının fikri ve istekleri

- Klavye artık görsel olarak birçok işe yarıyor:
  - tuş ışıkları (efektler, tuş başına renk, canlı katman);
  - dalgalar ve geçişler;
  - ekran kartları ve görseller;
  - odak zamanlayıcısı.
- Claude Code, Codex gibi kodlama ajanları da bunları kullanabilmeli:
  - ekrana metin ya da görsel göndermek;
  - onay, ret, hata, bekleme ya da duygu durumu gibi bildirimleri renk ve dalgayla vermek;
  - odak modunu başlatıp takip etmek.
- **Kullanıcının istekleri** (aynı gün, ilk önerinin üstüne):
  1. **Tek, sağlam bir yapı.** "API + CLI + MCP" gibi yan yana birden çok yapı istemiyor.
  2. **Ajanın doğal olarak kullanacağı bir şey.** Ajan bir programı açıp tek tek tıklamamalı; terminalden ya da kendi doğal yolundan rahatça kullanmalı.
  3. **Uygulamayı açmadan çalışmalı.** Örnek: kullanıcı Claude Code'un masaüstü sürümünü kullanıyor. Ajan, Keymotion'ın penceresi (`.exe`) açılmadan klavyeyi kullanabilmeli.
  4. **Davranışı ajan kendisi belirlemeli.** Hazır kalıplar olacak, ama ajan da bir kullanıcı gibi istediğini yapabilmeli. "Bekleme sarıdır" gibi sabit kurallar ajanın davranışını yanlış yönlendirebilir.
  5. **Yalnızca bir beceri (skill) dosyası yetmez,** bağlamı da şişirmemeli.
  6. **Görseller:** Ajanların durmadan görsel göndermesi ekranın belleğini yıpratır ve şişirir. Sistem yapılırken kontroller ve testlerle bir yol çizilecek; bugünün konusu değil.

## 2. Öneri: tek motor, tek CLI

### 2.1 Neden CLI

Ajanın tek ve asıl arayüzü bir **komut satırı aracı** olmalı (adı açık; örnek olarak `klavye`).

- **Ajanların en doğal dili kabuk.** Claude Code (masaüstü dahil), Codex, Cursor ve benzerleri komut çalıştırabiliyor. Ek bir kurulum ya da bağlantı ayarı gerekmez.
- **Bağlamı şişirmez.** Araç kullanılana kadar hiçbir yer kaplamaz. Ajan ihtiyaç duyunca `klavye --help` ya da `klavye rehber` ile öğrenir.
- **Herkes aynı yolu kullanır:** ajanlar, kullanıcının kendi betikleri, git kancaları, CI. Ajan kancaları (ör. Claude Code'un "izin bekliyor" olayı) da ayrı bir yapı değil; sadece bu aracı çağıran birer satır.
- MCP ve "beceri" ayrı yapılar olmaz:
  - **MCP:** Kabuğu olmayan bir istemci için ileride gerekirse, aynı aracın üstüne ince bir sarmalayıcı olur.
  - **Beceri / AGENTS.md:** Bir iki satırlık bir işaret yeter: "klavye aracı var, `klavye rehber` ile bak". Asıl bilgi aracın kendi yardımında durur.

### 2.2 Motor: klavyeyi tek sahip sürer

- Klavyeye yine tek bir süreç yazmalı. İki program aynı anda HID'e yazarsa çerçeveler karışır, ve uygulamanın kurduğu düzen devre dışı kalır:
  - canlı katmanlar;
  - yükleme bütçesi;
  - "Ekranda ne görünür?" sırası;
  - firmware, bootloader ve OTA yasakları.
- Bu süreç **motor**dur: bugünkü cihaz aktörü, ışık katmanları, odak zamanlayıcısı ve canlı mod.
- **Penceresiz çalışır.** CLI motoru bulamazsa kendisi arka planda başlatır; kullanıcı hiçbir pencere görmez. Keymotion penceresi açılınca o da aynı motora bağlanır. İnsan için pencere, ajan için CLI var; ikisinin arkasında aynı motor çalışır.
- **CLI ile motor arasındaki bağ:** Windows'ta adlandırılmış kanal (named pipe) üzerinden kısa JSON mesajları. Ağ portu yok. Kanal kullanıcıya özel olduğu için başka kullanıcılar ve uzaktaki makineler erişemez.
- **Bugünkü kodla bağlantı:**
  - Cihaz aktörü uygulamanın içinde (`app/src-tauri/src/device.rs`).
  - `kb` (kb-cli) bugün HID'i kendisi açan bir geliştirici aracı.
  - İki yol var:
    1. İlk adım olarak uygulamanın penceresiz modu (tepside, WebView'sız) motor olur. P3'teki arka plan servisi işiyle örtüşür.
    2. Gerekirse sonra motor ayrı bir crate ve küçük bir süreç olarak ayrılır.
  - Hangisinin olacağı uygulama zamanında ölçülerek karar verilir: bellek, açılış süresi.

### 2.3 Ajanın serbestliği: ilkel komutlar, sahneler, kendi kalıpları

Ajan bir kullanıcı gibi davranabilmeli. Bunun için araç üç düzey verir:

1. **İlkel komutlar.** Her şeyi yapabilen küçük parçalar:
   - bütün klavyeyi ya da seçilen tuşları renklendirmek;
   - degrade, dalga, nabız, yanıp sönme, süre ve yön vermek;
   - ekrana metin kartı göndermek;
   - odak zamanlayıcısını başlatmak ya da durdurmak.
2. **Sahne dosyası.** Ajan kendi animasyonunu JSON ile tarif eder: hangi tuş, hangi renk, hangi zamanda; döngü ya da tek sefer. Örneğin "R tuşu yanıp sönsün, F1–F12 soldan sağa dolsun".
3. **Kalıplar.** Hazır olanlar gelir (ör. `bekliyor`, `basarili`, `hata`), ama zorunlu değil:
   - Ajan kendi kalıbını kaydedip adıyla çağırabilir.
   - Kullanıcı bir kalıbın görünüşünü uygulamadan değiştirebilir.

**Ajanın klavyeyi görebilmesi:**
- `klavye tuslar --json`: tuş adları ve konumları (model dosyasındaki geometri). Ajan "Esc", "F5", "WASD" ya da "üst sıra" diye tuşları adlandırabilir.
- `klavye durum --json`: şu an ne gösteriliyor, bütçe ne kadar, zamanlayıcı hangi durumda, başka bir ajanın sinyali var mı.

### 2.4 Katmanlar: ajan bir şeyi bozamaz

- Ajanın her ışığı kendi **ajan katmanında** durur ve bir süresi vardır. Süre bitince ya da `klavye birak` denince ışık alttaki hâle döner: zamanlayıcıya, kullanıcının kendi ışığına ya da kayıtlı efekte. Ders 45: bitince ne olacağı baştan belli.
- Kalıcı bir şey (kayıtlı efekt, tuş ataması) ajan katmanından değiştirilemez.
- **Öncelik kararı gerekecek.** Örneğin ajan sinyali zamanlayıcının üstünde mi olmalı? Muhtemel cevap: kısa ve dikkat isteyen sinyaller üstte durur, sonra önceki hâle dönülür.
- **Birden çok ajan:** Her komut kimden geldiğini söyler (`--kimden claude-code` ya da bir ortam değişkeni). Motor sırayı yönetir.

### 2.5 Işık ucuz, ekran pahalı

- **Tuş ışığı:** RAM'e gider (`04 20`, kalıcı belleğe yazmaz), anında ve sınırsızdır. Ajanın asıl ifade alanı burası.
- **Ekran:** Her güncelleme kalıcı belleğe yazar. Kart yaklaşık 2 sn, büyük animasyon 37 sn'ye kadar sürer; günlük bütçeden düşer (DURUM maddeler 27, 32).
  - Ajanın ekran kullanımı bütçeye ve ajan başına bir sınıra tabidir.
  - Aynı içerik ikinci kez yüklenmez.
  - Görsel gönderme kullanıcının iznine bağlıdır.
  - Ayrıntılı kurallar sistem yapılırken kontrol ve testlerle belirlenecek (kullanıcının isteği 6).

### 2.6 Kullanıcının kontrolü ve güvenlik

- Uygulamada **Araçlar → Ajanlar** sayfası olur. Kullanıcı buradan şunları yönetir:
  - ajan aracı açık mı kapalı mı;
  - hangi ajanların bağlanabildiği;
  - ekrana yazma izni ve sınırı;
  - kalıpların görünüşü;
  - "rahatsız etme" saatleri;
  - son sinyallerin günlüğü (hangi ajan, ne zaman, ne gönderdi).
- Araçta firmware, bootloader, BLE OTA ya da kalıcı belleğe doğrudan yazan komut **hiç olmaz**. Korumalar motorda kalır (`mimari.md` §7).

### 2.7 Örnek: bir ajanın kullanımı

```text
klavye isik --renk "#ffb020" --nabiz 1.6sn --sure 60sn       # onay bekliyorum
klavye isik --tuslar F1-F12 --dalga soldan-saga --renk yesil --bir-kez   # testler geçti
klavye sahne kutlama.json                                    # ajanın kendi animasyonu
klavye kalip kaydet dusunuyorum dusunuyorum.json             # kendi kalıbı
klavye ekran yazi "Derleme bitti · 4 dk"                     # seyrek, bütçeli
klavye odak baslat 25
klavye birak                                                 # ajan katmanını kaldır
```

(Komut ve seçenek adları örnektir; Türkçe mi İngilizce mi olacağı açık soru.)

## 3. Açık sorular

- **Aracın adı ve dili:** `klavye` mi, bugünkü `kb` mi genişler? Komutlar Türkçe mi, İngilizce mi, yoksa ikisi birden mi?
- **Motor nerede çalışır?** Uygulamanın penceresiz modu mu, yoksa ayrı küçük bir süreç mi (§2.2)?
- **Öncelikler:** Ajan katmanı ile odak zamanlayıcısı, şarkı ve LED canlı katmanı nasıl sıralanır?
- **Ekran kuralları:** Ajan başına sınır, izin ve önizleme (kullanıcının isteği 6; testlerle).
- **Başka sistemler ve klavyeler:** macOS ve Linux sonra. Diğer klavye modellerinde de (P4) aynı araç çalışmalı; tuş adları ve geometri model dosyasından gelir.
