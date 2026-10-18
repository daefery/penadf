<p align="center">
  <img src="docs/assets/hero.png" alt="PenaDF dengan contoh kontrak sewa fiktif: klausul harga distabilo, catatan tempel, dan tanda tangan." width="100%">
</p>

<p align="center">
  <a href="https://github.com/daefery/penadf/releases/latest/download/PenaDF-Setup.exe"><img alt="Unduh pemasang" src="https://img.shields.io/badge/Unduh-Pemasang%20.exe-C41E3A?style=for-the-badge&logo=windows&logoColor=white&labelColor=C41E3A&color=1E2230"></a>
  <a href="https://github.com/daefery/penadf/releases/latest/download/PenaDF-windows-x64.zip"><img alt="Unduh versi portabel" src="https://img.shields.io/badge/Unduh-Portabel%20.zip-1E2230?style=for-the-badge&logo=windows&logoColor=white"></a>
</p>

<p align="center"><a href="README.md">Read in English</a></p>

# PenaDF

Aplikasi PDF kecil untuk Windows yang bekerja tanpa internet. Gratis dan open source.

Buka, baca, beri komentar, tanda tangani, isi formulir, gabung dan pisah halaman, bandingkan dua
versi, dan kunci PDF dengan kata sandi. Semua berjalan di PC Anda. File Anda tidak dikirim ke
mana pun.

## Cara memasang

![Empat langkah pemasang: sambutan, lisensi dan privasi, folder, selesai.](docs/assets/installer.png)

1. Unduh dan jalankan [`PenaDF-Setup.exe`](https://github.com/daefery/penadf/releases/latest/download/PenaDF-Setup.exe).
   Aplikasi terpasang untuk akun Windows Anda saja, jadi tidak perlu hak administrator.
   Jika muncul "Windows protected your PC", pilih **More info → Run anyway** (pemasang belum
   ditandatangani secara digital).
2. Baca ringkasan lisensi dan privasi, lalu klik **Saya Setuju**.
3. Di halaman terakhir, centang **Jadikan PenaDF aplikasi PDF utama**. Settings Windows akan
   terbuka di halaman PenaDF. Klik kotak di samping **.pdf**, lalu pilih **PenaDF**.
   Windows tidak mengizinkan aplikasi mana pun mengatur ini sendiri, jadi pilihan ada di tangan
   Anda. Bisa juga dilakukan nanti lewat **Tools → Make PenaDF the default PDF app**.
4. Untuk menghapus: **Settings → Apps → Installed apps → PenaDF**.

Butuh Windows 10 atau 11, 64-bit, dan ruang sekitar 16 MB. Tanpa akun, tanpa internet.

## Yang bisa dilakukan

| Pekerjaan | Di PenaDF |
|---|---|
| Membuat PDF | Dari gambar (PNG, JPG, dan lainnya) dan file teks. File Word, Excel, PowerPoint diubah lewat Microsoft Office atau LibreOffice jika sudah terpasang |
| Tanda tangan | Gambar tanda tangan sekali, lalu tempelkan dengan menyeret kotak. Tersimpan untuk dipakai lagi |
| Komentar | Stabilo, garis bawah, coret, catatan tempel, gambar bebas |
| Isi formulir | Kolom teks dan kotak centang, bisa dikunci setelah diisi |
| Atur halaman | Putar, hapus, pindah urutan, sisipkan, potong tepi, gabung beberapa file, pisah jadi beberapa file |
| Bandingkan | Lihat baris teks yang berubah antara dua versi PDF, lengkap dengan nomor halaman |
| Kunci PDF | Kata sandi untuk membuka dan batasan cetak, salin, ubah (AES-256) |
| Ekspor | Halaman ke PNG/JPG, teks ke .txt, PDF ke halaman web .html |
| Tab | Setiap PDF terbuka di tab sendiri. Klik dua kali PDF lain di Explorer, langsung jadi tab baru |

Tanda tangan di PenaDF adalah gambar tanda tangan yang terlihat di halaman. Ini bukan tanda
tangan elektronik tersertifikasi (PSrE).

Belum tersedia: mengedit teks yang sudah ada di PDF, OCR untuk hasil scan, dan PDF ke Word.

## Privasi

PenaDF tidak terhubung ke internet: tanpa akun, tanpa iklan, tanpa pelacakan. Yang disimpan
hanya tanda tangan Anda (jika Anda memilih menyimpannya) di folder `%APPDATA%\PenaDF`.
Detailnya di [PRIVACY.md](PRIVACY.md).

## Lapor masalah atau usul fitur

Buka "Issues" di halaman GitHub proyek ini. Tulis apa yang Anda lakukan, apa yang terjadi, dan
versi Windows Anda.

## Lisensi

Apache License 2.0. Lisensi komponen pihak ketiga ada di `THIRD-PARTY-NOTICES.txt`.
