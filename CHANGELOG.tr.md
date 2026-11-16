# Sürüm notları

Keymotion'daki önemli değişiklikler burada listelenir. İngilizcesi: [CHANGELOG.md](CHANGELOG.md).

1.0.0'dan itibaren her sürüm GitHub'da ve Microsoft Store'da aynıdır; ikisi
için tek bir değişiklik listesi vardır. Windows, Store'dan kurulan kopyanın
sürümünü sonuna her zaman 0 olan dördüncü bir sayıyla gösterir (1.0.0, 1.0.0.0
olur). Bir sürüm ikisinde aynı gün çıkar: GitHub sürümü, Microsoft Store
kopyasını denetleyene kadar bekler. Yalnızca acil bir düzeltme önce GitHub'da
çıkabilir ve bunu belirtir. Deneme sürümleri ("-rc") yalnızca GitHub'dadır.
Yalnızca birini ilgilendiren bir değişiklik bunu belirtir.

## [Yayımlanmamış]

### Eklenenler

- **Oyun oturumu:** Araçlar › **Oyun oturumu** oynadığınızı fark eder.
  Oyunlar başlatıcılarınızın listelerinden (Steam, Epic Games, GOG, Xbox,
  Riot Games, Xbox Game Bar) ve sizin sözünüzden gelir (**Oyun ekle**,
  **Oyun değil**, **Geri al**); birkaç dakika ekranı kaplayan bir program için
  "Bir oyun mu?" diye sorulur. Bir oyun 15 saniye
  önde kalınca oturum başlar ve oyun açık kaldıkça, Alt+Tab'da da sürer.
  Canlı ekran açıksa başlarken klavyenin ekranına bir kart, 5 dakika ya da
  daha uzun bir oturum bitince bir özet (süre, ekran kartının yükü ve
  sıcaklığı, işlemcinin yükü, bugünün toplamı) gider; oyun sürerken ekrana
  yeni bir şey gelmez. Oyunun tuşları (WASD, MOBA, 1–6 ya da sizin
  seçtikleriniz) klavyeye bir şey kaydedilmeden yanabilir. Geçmiş bu
  bilgisayarda kalır. "Kendiliğinden güncelle" oyunun bitmesini bekler.
- **League of Legends ışıkları:** maç sırasında, oyunun bilgisayarınızda
  sunduğu veriden: can 1–0'da, mana F1–F12'de; ölünce koyu kırmızı ve
  yeniden doğma sayacı; seviye atlama, öldürme, as, ejder, Baron ve Herald
  için kısa parlamalar; sonda zafer ya da yenilgi. Özet kartı sonucu, K/D/A'yı
  ve minyon sayısını gösterir. Rakip ya da hedef sayaçları yok.
- **Uygulamaya göre profil:** Profiller › **Uygulamaya göre** bir programı
  ya da bütün oyunları bir profile bağlar. Varsayılan olarak yalnız tuş
  başına renk tasarımının ışığı değişir, klavyeye bir şey kaydedilmez; bir
  kural tuş atamasını ve makroları da kaydedebilir, günde en çok 20 kez.
  Oyunun profili oyun boyunca kalır. Kurallar program güncellendikten sonra
  da tutar (Microsoft Store uygulamaları, Discord, Roblox). Simge menüsünden
  duraklatılabilir.
- **Oyun sürerken klavyeye hiçbir şey gönderilmesin:** oyun boyunca Keymotion
  klavyeye ışık, kart ya da profil göndermez, klavye kendi ışığında kalır;
  hile önleyicisi bundan rahatsız olabilecek oyunlar için.

### Değişenler

- Yazmadan önce resmî yazılımı ya da SignalRGB'yi bulmak için kullanılan
  çalışan programlar listesi artık hiçbir programı açmıyor.

## [1.1.0] - 2026-11-16

Keymotion artık yeni sürüm çıkınca haber veriyor ve isterseniz kendiliğinden
kuruyor. Microsoft Store'da da var.

### Eklenenler

- **Microsoft Store:** Keymotion 16 Kasım 2026'dan beri Microsoft Store'da.
  Oradaki kopyayı Microsoft imzalar; Windows onu uyarı göstermeden kurar.
  Sürüm sayfası, README ve site, GitHub indirme düğmesinin yanında
  Microsoft'un "Microsoft'tan alın" rozetini gösterir.
- **Güncelleme bildirimleri:** Yeni sürüm çıkınca, her sürüm için bir kez,
  **Şimdi güncelle** ve **Sonra** düğmeli bir Windows bildirimi bunu söyler.
  Saatin yanındaki Keymotion simgesinin menüsü **Güncelleme var** satırıyla
  başlar ve simgede küçük bir nokta görünür; pencerenin sol altındaki kart
  ve **Ayarlar**'ın yanındaki nokta da söyler. Önceden yeni sürüm yalnızca
  pencerenin içinde görünüyordu; Keymotion saatin yanında çalışırken fark
  edilmiyordu.
- **Kendiliğinden güncelleme:** Ayarlar › Hakkında › **Güncellemeler**'de
  tek anahtarın yerine üç seçenek var. **Kendiliğinden güncelle** yeni
  sürümü indirir ve bilgisayar kullanılmadığı bir anda kurar; klavyeye
  yazarken, odak zamanlayıcısı çalışırken ya da klavyeye kaydedilmemiş
  değişiklik varken asla. **Haber ver** (varsayılan) size söyler, ne zaman
  kuracağınızı siz seçersiniz; **Kapalı** kendiliğinden hiç bakmaz. Eski
  anahtar kapalıysa seçim **Kapalı** olur.
- Güncellemeden sonra bir Windows bildirimi hangi sürümün kurulduğunu
  **Neler yeni** düğmesiyle söyler; Keymotion da eskisi gibi açılır:
  penceresiyle ya da saatin yanında.

### Değişenler

- **İki kopya birden kuruluysa:** Bir bilgisayarda hem GitHub'dan hem
  Microsoft Store'dan kurulan Keymotion varsa GitHub kopyası artık açılınca
  Store kopyasını açar; bilgisayar açılınca başlama da Store kopyasına geçer.
  Önceden ikisi de bilgisayar açılınca başlamaya çalışır, önce davranan
  çalışırdı. Store kopyası Ayarlar › Hakkında bölümünde bir "GitHub kopyası"
  satırı gösterir; satırdaki düğme Windows'un Yüklü uygulamalar sayfasını
  açar ve GitHub kopyası oradan kaldırılabilir. İki kopya ayarları paylaştığı
  için ayarlar kalır.
- Keymotion açıkken bilgisayar açılışında ikinci kez başlatılırsa pencereyi
  artık öne getirmez.

### Düzeltilenler

- **Microsoft Store:** Store yeni sürümü kurarken Windows çalışan
  Keymotion'ı kapatır. Keymotion artık ardından kendiliğinden, saatin
  yanında yeniden açılır. Önceden bir sonraki oturum açılışına kadar kapalı
  kalıyordu.

## [1.0.0] - 2026-11-15

Keymotion 1.0. Uygulama 0.1.0 ile aynı, Microsoft Store'a hazır. Microsoft
Store 0 ile başlayan sürüm numarası kabul etmediği için bu sürüm 1.0.0.

### Eklenenler

- **Microsoft Store sürümü:** Aynı uygulama, Microsoft Store için
  paketlendi. Store'daki sürümü Microsoft imzalar; Windows onu uyarı
  göstermeden kurar. Store'dan kurulan Keymotion'ı Store günceller (kendi
  güncelleme denetimi kapalı kalır), bilgisayar açılınca Windows'un başlangıç
  ayarlarıyla başlar ve bildirimlerinde Store uygulamasının adı ve logosu
  görünür. GitHub'daki sürüm eskisi gibi çalışır.

## [0.1.0] - 2026-11-12

İlk sürüm. Windows'ta AJAZZ AK980 PRO'yu destekler.

### Eklenenler

- **Bağlantı:** Klavye USB kablo, 2.4 GHz alıcı ve Bluetooth bağlantısında
  tanınır; takıldığı ya da çıkarıldığı fark edilir. Bağlantı sayfası her modda
  neler yapılabildiğini gösterir; ayarlar için USB kablo gerekir. Bluetooth
  bağlantısında pil düzeyi gösterilir.
- **Genel bakış:** Sayfa genişliğince, ölçülerek çizilmiş 97 tuşlu düzen ve her
  tuşun üzerindeki harfler; ardından klavye ekranında şu an görünen görsel ve
  her araç için bir kutucuk: ışık, odak zamanlayıcısı (buradan
  başlatılabilir), sıradaki hatırlatma, sıradaki toplantı, canlı çubuklarıyla
  sistem göstergeleri ve profil. Ayrıca bağlantı modu, pil ve klavye saati;
  saat, klavye bağlanınca kendiliğinden ayarlanır.
- **Işıklar:** Renk, parlaklık, hız ve yön ayarıyla 19 hazır efekt; klavyenin
  renklerinin canlı yansıması; fırça ve seçim araçlarıyla tuş başına renk,
  hazır desenler, canlı önizleme ve klavyeye kaydetme.
- **Renk kalibrasyonu:** Klavyede gösterilen test desenleriyle gama ve beyaz
  dengesi ayarı.
- **Tuş atama:** Temel katman ve Fn katmanı. Bir tuşa başka bir tuş, kısayol,
  medya tuşu, fare düğmesi ya da kaydırma atanabilir veya tuş devre dışı
  bırakılabilir; bir katman fabrika düzenine sıfırlanabilir.
- **Makrolar:** Tuşlara basılarak kaydedilir, adım adım düzenlenir (bekleme
  süreleri, sıra, fare tıklamaları) ve bir oynatma biçimiyle tuşlara atanır;
  klavye belleğinde ne kadar yer kaldığı görünür.
- **Ekran stüdyosu:** 240×135 ekran için görseller, GIF'ler ve videolar
  (kırpma, süre), yazı kartları ve slayt gösterileri; yüklemeden önce
  önizleme, kütüphane ve fabrika animasyonunu geri yükleme. "Işıkları
  eşleştir", görseldeki renklerden tuş renkleri tasarlar.
- **Canlı ekran:** Kendiliğinden güncellenen kartlar (çalan şarkı, odak
  zamanlayıcısının aşamaları, yaklaşan toplantılar); günlük yükleme sınırı,
  ekranda neyin görüneceğini belirleyen açık bir sıra ve ardından kendi
  görselinize dönüş. Odak zamanlayıcısı ve şarkı, son yükleme hakkını
  hatırlatmalara ve toplantılara bırakır. Sınırı bekleyen bir kart, klavyede
  o an görünenden ayrı gösterilir ve beklediği belirtilir; "Gönderme" o kartı
  sıradan çıkarır.
- **Takvim:** Google, Outlook ve iCloud takvimleri paylaşım adresleriyle
  eklenir (yalnızca okunur); toplantıdan önce ekranda bir kart çıkar,
  başlamasına 1 dk kala bir tuş yanıp söner.
- **Hatırlatıcılar:** Bir kez, her gün ya da çalışma saatlerinde aralıklarla;
  tuşlarda bir renk dalgası, isteğe bağlı olarak ekranda bir kart ve ses.
  "Dene" kartı da gösterir; sayfa, bir kartın neden gösterilmediğini belirtir
  (yükleme sınırı, canlı ekranın kapalı olması, USB kablonun takılı olmaması,
  toplantı kartı).
- **Sistem göstergeleri:** İşlemci, ekran kartı ve bellek yükü tuş
  sıralarında çubuk olarak, ses düzeyi işlev tuşlarında, mikrofonun sessize
  alındığı seçtiğiniz bir tuşta gösterilir.
- **Odak zamanlayıcısı:** Odak ve mola bloklarını istediğiniz uzunlukta ve
  istediğiniz sırayla dizerek kendi sıranızı kurarsınız (blokların yerini
  sürükleyerek değiştirebilirsiniz); sıra bir kez, birkaç kez ya da siz
  durdurana kadar çalışır. Başlangıçta "25 / 5 klasik" seçilidir; eski
  ayarlar, karşılık geldikleri sıraya dönüştürülür. Bütün klavye bloğun
  rengiyle dolar ve süre azaldıkça boşalır; her blok için ekranda bir kart
  çıkar.
- **Kurulum:** Windows için İngilizce ve Türkçe kurulum programı; uygulamayı
  yalnızca sizin hesabınıza kurar, Keymotion simgesini ve sade bir karşılama
  metnini gösterir, yönetici izni gerektirmez. Kaldırırken "Ayarlarımı ve
  kayıtlarımı da sil" işaretlenmezse ayarlarınız korunur; kaldırma işlemi
  Windows'la başlatma kaydını ve bildirimler için kaydedilen adı da siler.
- **Sorun bildirme:** Ayarlar › Hakkında › Geri bildirim, beklenmeyen bir
  hatadan sonra çıkan göze batmayan uyarı ve uygulama çöktükten sonra bir kez
  çıkan uyarı, sorun bildirme penceresini açar; pencere gönderilecek bilgileri
  önce gösterir. "GitHub'da bildir" bilgileri kopyalar ve projenin sorun
  formunu varsayılan tarayıcıda açar; uygulama kendiliğinden hiçbir şey
  göndermez. Henüz desteklenmeyen bir SONiX ya da AJAZZ klavye için "Destek
  iste" çıkar; isteğe klavyenin Windows'tan okunan USB kimlikleri ve HID
  tanımları eklenir (klavyeye hiçbir şey yazılmaz).
- **Keymotion animasyonu:** Klavye ekranı için Keymotion'ın kendi animasyonu,
  kütüphanede hazır: logodaki "K" çizgi çizgi çizilir, ardından Keymotion
  yazısı görünür (37 kare, yüklemesi yaklaşık 1,5 dk). Fabrika animasyonunun
  yedeği yoksa önerilir.
- **Saatin yanındaki simge:** Pencereyi kapattığınızda Keymotion saatin
  yanında çalışmaya devam eder; böylece odak zamanlayıcısı, hatırlatıcılar,
  takvim kartları, canlı ekran ve tuşlardaki göstergeler durmaz. Simgenin
  menüsü klavyeyi ve pilini gösterir, odak zamanlayıcısını yönetir (aşama ve
  kalan dakika, duraklatma, sonraki aşamaya geçme, durdurma), sıradaki
  hatırlatmayı gösterir, hatırlatıcıları ve tuşlardaki göstergeleri açıp
  kapatır. Menüden bir profilin yükleme onayı da açılır ve uygulamadan
  çıkılır. Bu davranış Ayarlar › Kapatma ve açılış bölümünden kapatılabilir;
  aynı yerden Keymotion'ın Windows açılınca penceresiz başlaması da
  seçilebilir. Uygulama
  açıldıktan sonra pencere ilk kez kapatıldığında, Keymotion'ın hâlâ
  çalıştığını bildiren bir uyarı görünür.
- **Güncellemeler:** Ayarlar › Hakkında bölümünden GitHub'da yeni sürüm olup
  olmadığı denetlenir; uygulama da açıldığında günde bir kez kendiliğinden
  bakar (bu kapatılabilir). Yeni sürüm bulununca sayfanın üstünde bir şerit
  görünür; "Kur ve yeniden başlat"ı seçmediğiniz sürece hiçbir şey kurulmaz.
  İndirilen dosya uygulamanın kendi anahtarıyla doğrulanır; kurulum, klavyeye
  süren bir yazma işleminin bitmesini bekler.
- **Açılış animasyonu:** Pencere açılırken logodaki "K" çizgi çizgi çizilir ve
  Keymotion adı açılır, ardından logo soldaki menüde kendi yerine geçer
  (uygulama hazırlanırken yaklaşık bir saniye). Keymotion Windows açılınca
  penceresiz başladığında ya da saatin yanındaki simgeden yeniden açıldığında
  oynatılmaz; bir tıklama ya da tuşa basma animasyonu bitirir; Windows'ta
  animasyon efektleri kapalıysa yalnızca solarak kaybolur.
- Sayfadan ayrılırken ya da uygulama kapatılırken kaydedilmemiş değişiklikler
  için koruma.
- Türkçe ve İngilizce arayüz, açık ve koyu tema.
- Proje: MIT/Apache-2.0 lisansı, NOTICE (resmî olmayan proje, kullanım riski
  size aittir), üçüncü taraf lisans bildirimleri ve bozuk karakterler, üretici
  dosyaları, sürümler ve bağımlılıklar için otomatik denetimler.
- **Belgeler:** cebersaban44-rgb.github.io/keymotion adresinde Türkçe ve
  İngilizce kullanım kılavuzu: uygulamanın her sayfası, bir şey ters giderse ne
  yapılacağı, desteklenen ve sırada olabilecek klavyeler, geliştiriciler için
  sayfalar. Ayarlar › Hakkında'dan açılır.

[Yayımlanmamış]: https://github.com/cebersaban44-rgb/keymotion/compare/v1.1.0...develop
[1.1.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v1.1.0
[1.0.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v1.0.0
[0.1.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v0.1.0
