# ZeroScript — Package & Dependency System

81. Package & Dependency System
ZeroScript membedakan:
Module → file .zs di dalam project.
Package → kumpulan module yang bisa digunakan project lain.
Dependency → package yang dibutuhkan project.
Contoh struktur:
my-app/
├── zeroproject.zs
├── main.zs
└── deps/
    └── http-utils/
        ├── package.zs
        └── src/
            └── client.zs
Import package:
import { request } from "http-utils/client"
Aturan awal:
Path ./ dan ../ → module lokal.
Path tanpa ./ atau ../ → dianggap package import.
Package harus tersedia di dependency project.
Package yang tidak ditemukan → ERROR.
Package tidak boleh mengakses file di luar boundary yang diberikan kepadanya secara diam-diam.
Package tetap menggunakan sistem module ZeroScript yang sama.
Package tidak menjadi global hanya karena terpasang di sistem.
Jadi:
import { tambah } from "./math"
= module lokal
import { request } from "http-utils/client"
= package dependency

---

82. Project Manifest

ZeroScript menggunakan file project.zson sebagai manifest project.

Contoh:

{
    name: "my-app",
    version: "1.0.0",

    dependencies: {
        "http-utils": "1.2.0",
        "database": "2.0.1"
    }
}

Aturan:

project.zson berada di root project.

project.zson menggunakan format ZSON (ZeroScript Object Notation).

project.zson berisi data konfigurasi dan tidak dieksekusi sebagai source code ZeroScript.

name wajib berupa string.

version wajib berupa string.

dependencies berupa object.

Nama package menjadi key pada dependencies.

Versi dependency ditulis sebagai string.

Dependency yang tercantum menjadi dependency project.

Package manager menggunakan project.zson untuk menentukan dependency yang harus tersedia di .zsc_m/.

project.zson tidak dapat di-import menggunakan import.

Konfigurasi compiler atau tooling yang lebih spesifik dapat menggunakan file konfigurasi terpisah jika diperlukan.


Contoh struktur project:

my-app/
├── project.zson
├── main.zs
└── .zsc_m/
    ├── http-utils/
    └── database/

Dengan demikian, ZeroScript tidak membutuhkan package.json sebagai manifest project. ZSON menjadi format konfigurasi/data native ZeroScript.

---

83. Package Versioning

Setiap dependency pada "project.zson" memiliki version selector yang menentukan versi package yang dapat digunakan.

ZeroScript menggunakan format versi Semantic Versioning (SemVer):

MAJOR.MINOR.PATCH

Contoh:

1.2.0
1.2.10
1.3.2
2.0.0

Version Selector

ZeroScript menggunakan exact version dan "x" sebagai wildcard.

dependencies: {
    "http-utils": "1.2.0"
}

Memilih versi tepat "1.2.0".

dependencies: {
    "http-utils": "1.2.x"
}

Memilih versi terbaru yang memenuhi pola "1.2.*".

Misalnya tersedia:

1.2.0
1.2.5
1.2.10
1.3.2

Maka "1.2.x" memilih:

1.2.10

Versi "1.3.2" tidak memenuhi selector tersebut.

dependencies: {
    "http-utils": "1.x"
}

Memilih versi terbaru dalam major "1".

Dengan versi:

1.2.10
1.3.0
1.3.2
2.0.0

maka "1.x" memilih:

1.3.2

dependencies: {
    "http-utils": "x"
}

Memilih versi terbaru yang tersedia dari package tersebut.

Aturan

- Exact version ("1.2.0") → hanya versi tersebut yang diterima.
- "1.2.x" → "MAJOR" dan "MINOR" harus cocok; "PATCH" bebas.
- "1.x" → "MAJOR" harus cocok; "MINOR" dan "PATCH" bebas.
- "x" → semua versi kompatibel dengan selector dan versi terbaru dipilih.
- "x" bukan bagian dari versi SemVer package; "x" hanya digunakan sebagai version selector.
- Selector selalu memilih versi terbaru yang memenuhi pola selector.
- Jika tidak ada versi yang memenuhi selector → "ERROR".
- ZeroScript tidak menggunakan operator range seperti "^", "~", ">=",
 Versi yang telah dipilih dapat dicatat oleh lockfile agar instalasi berikutnya tetap menggunakan versi yang sama.

Contoh:

{
    name: "my-app",
    version: "1.0.0",

    dependencies: {
        "http-utils": "1.2.x",
        "database": "x"
    }
}

Jika versi tersedia:

http-utils:
1.2.0
1.2.5
1.2.10
1.3.2

database:
1.0.0
1.5.0
2.0.0

Maka resolver memilih:

http-utils → 1.2.10
database → 2.0.0

Dengan demikian, sistem versioning ZeroScript tetap sederhana: exact version untuk kontrol penuh, "x" untuk fleksibilitas, dan tanpa range operator yang kompleks.

------

84. Dependency Lockfile

Kita tambahkan lockfile supaya dependency yang sudah dipilih tidak berubah-ubah hanya karena ada versi baru.

Nama file:

project.lock.zson

Contoh project.zson:

{
    name: "my-app",
    version: "1.0.0",

    dependencies: {
        "http-utils": "1.2.x",
        "database": "x"
    }
}

Setelah resolver memilih versi:

{
    packages: {
        "http-utils": "1.2.10",
        "database": "2.0.0"
    }
}

Aturan:

project.zson menentukan versi yang diperbolehkan.

project.lock.zson mencatat versi konkret yang dipilih.

Jika lockfile tersedia dan masih valid, package manager menggunakan versi yang tercatat.

Dependency tidak otomatis naik versi hanya karena versi terbaru tersedia.

Jika lockfile dihapus, resolver dapat memilih versi terbaru yang memenuhi selector pada project.zson.

Perubahan dependency di project.zson dapat menyebabkan lockfile diperbarui.

Lockfile bukan source code dan tidak dapat di-import.

Lockfile boleh di-commit ke repository agar environment development/build mendapatkan dependency yang sama.


Jadi:

project.zson
    ↓
"1.2.x"
    ↓
resolver
    ↓
project.lock.zson
    ↓
"1.2.10"

project.zson = aturan versi.
project.lock.zson = hasil versi yang benar-benar dipakai.

---

85. Dependency Installation

Package manager ZeroScript menyediakan operasi untuk memasang dependency berdasarkan project.zson.

Perintah dasar:

zpm install

zpm = ZeroScript Package Manager.

Alurnya:

project.zson
    ↓
baca dependencies
    ↓
cek project.lock.zson
    ↓
resolve versi
    ↓
download package
    ↓
.zsc_m/
    ↓
update project.lock.zson

Contoh:

my-app/
├── project.zson
├── project.lock.zson
├── main.zs
└── .zsc_m/
    ├── http-utils/
    └── database/

Aturan:

zpm install membaca project.zson.

Jika lockfile valid, versi yang tercatat digunakan.

Jika belum ada lockfile, resolver memilih versi sesuai selector.

Package yang berhasil di-resolve disimpan di .zsc_m/.

Lockfile dibuat atau diperbarui setelah dependency berhasil dipasang.

Jika package tidak ditemukan atau tidak ada versi yang memenuhi selector → ERROR.

Package yang sudah tersedia dengan versi yang sesuai tidak perlu di-download ulang.

.zsc_m/ dikelola oleh package manager.


Jadi programmer cukup:

zpm install

lalu:

import { sesuatu } from "nama-package"

Tanpa perlu mengatur isi .zsc_m secara manual.

---

86. Project Scripts

ZeroScript Package Manager ("zpm") menyediakan sistem project scripts untuk menyimpan dan menjalankan perintah yang umum digunakan dalam sebuah project.

Script didefinisikan di dalam "project.zson" melalui properti "scripts".

Contoh:

{
    name: "my-app",
    version: "1.0.0",

    scripts: {
        start: "zsc index.zs"
    },

    dependencies: {
        "http-utils": "1.2.x"
    }
}

Script dapat dijalankan dengan:

zpm run start

Perintah tersebut menjalankan command yang didefinisikan pada script:

zsc index.zs

Aturan

- "scripts" harus berupa object.
- Nama script merupakan key string.
- Nilai script harus berupa string command.
- "zpm run <name>" menjalankan script dengan nama tersebut.
- Jika script tidak ditemukan → ERROR.
- Script dijalankan dari project root.
- Script dapat menjalankan "zsc", "zerovm", atau command lain yang tersedia pada environment.
- "zpm" tidak mengubah isi command yang ditulis programmer.
- Script dapat menerima argument tambahan.

Contoh:

zpm run start

zpm run start -- --debug

Script:

scripts: {
    start: "zsc index.zs"
}

Command tambahan diteruskan kepada command script.

Beberapa script dapat didefinisikan dalam satu project:

scripts: {
    start: "zsc index.zs",
    dev: "zerovm index.zs",
    test: "zsc tests/main.zs"
}

Sehingga dapat dijalankan:

zpm run start
zpm run dev
zpm run test

Project scripts merupakan fitur "zpm" dan bukan bagian dari bahasa pemrograman ZeroScript itu sendiri

----

87. Package Resolution During Execution

ZeroScript harus dapat menemukan package dependency yang telah di-install ketika source code dijalankan atau dikompilasi.

Package resolver menggunakan ".zsc_m/" pada project sebagai sumber package dependency.

Contoh struktur:

my-app/
├── project.zson
├── project.lock.zson
├── index.zs
└── .zsc_m/
    └── http-utils/
        └── ...

Source code dapat mengimpor package:

import { request } from "http-utils/client"

Ketika import tersebut diproses, ZeroScript mencari package "http-utils" pada dependency environment project.

Aturan

- Package harus tercantum pada "dependencies" di "project.zson".
- Package harus tersedia pada ".zsc_m/".
- Package yang tidak tercantum sebagai dependency → ERROR.
- Package yang tercantum tetapi belum ter-install → ERROR.
- Resolver menggunakan versi yang telah ditentukan oleh "project.lock.zson" jika lockfile valid.
- Package resolution tidak bergantung pada current working directory.
- ".zsc_m/" dikelola oleh "zpm" dan tidak menjadi global module store.
- Project yang berbeda memiliki dependency environment masing-masing.
- Dependency package dapat memiliki dependency lain sesuai aturan package/dependency system.
- Dependency resolution harus menghasilkan module yang deterministik berdasarkan manifest dan lockfile.

Contoh:

my-app/
├── project.zson
├── project.lock.zson
├── index.zs
└── .zsc_m/
    ├── http-utils/
    └── database/

Dengan:

dependencies: {
    "http-utils": "1.2.x",
    "database": "2.x"
}

Maka import package dari "index.zs" dapat menggunakan package tersebut selama package telah berhasil di-install dan tersedia pada ".zsc_m/".

Package resolution berlaku baik ketika source dijalankan secara langsung maupun ketika project dijalankan melalui project script.

88. Package Dependency Tree

Sebuah package dapat memiliki dependency terhadap package lain.

Dependency tersebut membentuk dependency tree, dengan project sebagai root.

Contoh:

my-app
├── http-utils
│   └── url-parser
└── database
    └── crypto-utils

Jika "my-app" memiliki:

dependencies: {
    "http-utils": "1.2.x",
    "database": "2.x"
}

dan "http-utils" membutuhkan "url-parser", maka "url-parser" menjadi dependency dari "http-utils".

Aturan

- Package dapat mendeklarasikan dependency miliknya sendiri.
- Dependency package harus diselesaikan sebelum package yang membutuhkannya dapat digunakan.
- Semua dependency yang diperlukan harus tersedia sebelum program dijalankan.
- Dependency dapat memiliki dependency lain secara rekursif.
- Dependency yang sama dapat digunakan oleh beberapa package.
- Resolver harus menghindari instalasi duplikat ketika versi yang sama dapat digunakan bersama.
- Konflik versi harus diselesaikan berdasarkan aturan version selector dan lockfile.
- Jika tidak terdapat versi yang memenuhi seluruh kebutuhan dependency → ERROR.
- Circular dependency antar-package → ERROR.
- Dependency tree harus dapat direproduksi menggunakan "project.lock.zson".

Contoh:

my-app
├── package-a ──┐
│               └── shared-utils@1.2.0
└── package-b ──┘

Jika "package-a" dan "package-b" membutuhkan "shared-utils@1.2.0", satu versi tersebut dapat digunakan bersama.

Namun jika:

package-a → shared-utils@1.2.x
package-b → shared-utils@2.x

dan kedua kebutuhan tidak dapat menggunakan versi yang sama, resolver harus mempertahankan versi yang diperlukan oleh masing-masing dependency sesuai aturan dependency resolution.

Dependency tree bukan bagian dari syntax bahasa ZeroScript dan dikelola oleh "zpm".

89. Dependency Scope

Setiap package memiliki dependency scope sendiri.

Dependency yang didefinisikan oleh sebuah package digunakan untuk memenuhi kebutuhan package tersebut dan tidak otomatis menjadi dependency langsung dari project.

Contoh:

my-app
└── http-utils
    └── url-parser

Jika "http-utils" membutuhkan "url-parser", maka "http-utils" dapat menggunakannya tanpa "my-app" harus mendefinisikan "url-parser" sebagai dependency langsung.

Aturan

- Project hanya memiliki akses langsung terhadap package yang didefinisikan sebagai dependency project.
- Dependency milik package lain tidak otomatis dapat di-import langsung oleh project.
- Package dapat meng-import dependency miliknya sendiri.
- Dependency transitif tetap menjadi bagian dari dependency tree.
- Jika project ingin menggunakan package transitif secara langsung, package tersebut harus ditambahkan sebagai dependency project.
- Dependency scope tidak memengaruhi module scope di dalam package.
- Package tidak dapat mengakses dependency milik package lain hanya karena package tersebut kebetulan telah ter-install.
- "zpm" bertanggung jawab menjaga dependency scope selama proses resolution dan installation.

Contoh:

{
    name: "my-app",
    version: "1.0.0",

    dependencies: {
        "http-utils": "1.2.x"
    }
}

Jika:

http-utils
└── url-parser

maka:

import { request } from "http-utils/client"

valid.

Sedangkan:

import { parse } from "url-parser"

akan menghasilkan ERROR jika "url-parser" hanya merupakan dependency milik "http-utils" dan tidak didefinisikan sebagai dependency langsung project.

Dependency scope dibuat eksplisit agar package tidak dapat bergantung pada package lain secara tidak langsung tanpa deklarasi.

90. Dependency Conflict Resolution

ZeroScript Package Manager ("zpm") harus menangani konflik versi ketika beberapa dependency membutuhkan package yang sama dengan version selector yang berbeda.

Contoh:

my-app
├── package-a
│   └── shared-utils@1.x
└── package-b
    └── shared-utils@2.x

Jika satu versi "shared-utils" tidak dapat memenuhi kedua kebutuhan tersebut, "zpm" dapat menggunakan beberapa versi secara terisolasi.

Aturan

- Setiap dependency harus menggunakan versi yang memenuhi version selector-nya.
- Jika satu versi dapat memenuhi beberapa kebutuhan dependency, versi tersebut dapat digunakan bersama.
- Jika kebutuhan versi berbeda dan tidak memiliki versi yang sama-sama memenuhi, dependency dapat menggunakan versi berbeda.
- Setiap versi dependency tetap berada dalam scope dependency yang membutuhkannya.
- Package tidak boleh secara otomatis menggunakan versi dependency milik package lain.
- Konflik yang tidak dapat diselesaikan → ERROR.
- Hasil resolution harus dicatat pada "project.lock.zson".
- Resolution yang telah terkunci harus menghasilkan dependency tree yang sama ketika project di-install kembali menggunakan lockfile yang valid.

Contoh:

my-app
├── package-a
│   └── shared-utils@1.5.0
└── package-b
    └── shared-utils@2.1.0

Kedua versi dapat digunakan secara terisolasi apabila diperlukan.

Jika:

package-a → shared-utils@1.x
package-b → shared-utils@1.x

dan "shared-utils@1.8.0" memenuhi keduanya, resolver dapat menggunakan satu versi:

my-app
├── package-a ──┐
│               └── shared-utils@1.8.0
└── package-b ──┘

"zpm" tidak boleh mengganti dependency ke versi yang tidak memenuhi selector hanya untuk menghindari konflik.

91. Dependency Removal

ZeroScript Package Manager ("zpm") menyediakan operasi untuk menghapus dependency yang tidak lagi digunakan oleh project.

Basic command:

zpm remove <package>

Contoh:

zpm remove http-utils

Perintah tersebut menghapus "http-utils" dari dependency langsung project.

Aturan

- "zpm remove <package>" menghapus package dari "dependencies" pada "project.zson".
- Dependency yang dihapus tidak lagi dianggap sebagai dependency langsung project.
- "zpm" memperbarui "project.lock.zson" setelah proses removal berhasil.
- Package yang tidak lagi diperlukan oleh dependency tree dapat dihapus dari ".zsc_m/".
- Package yang masih diperlukan oleh dependency lain tidak boleh dihapus.
- Dependency transitif tidak dapat dihapus secara langsung jika masih diperlukan oleh package lain.
- Menghapus package yang tidak terdaftar sebagai dependency project → ERROR.
- Setelah removal berhasil, source code yang masih meng-import package tersebut akan menghasilkan ERROR ketika dijalankan atau dikompilasi.

Contoh:

my-app/
├── project.zson
├── project.lock.zson
├── index.zs
└── .zsc_m/
    ├── http-utils/
    └── database/

Setelah:

zpm remove http-utils

Jika tidak ada dependency lain yang membutuhkan "http-utils", hasilnya menjadi:

my-app/
├── project.zson
├── project.lock.zson
├── index.zs
└── .zsc_m/
    └── database/

Jika "http-utils" masih dibutuhkan oleh dependency lain, package tersebut tetap berada di ".zsc_m/" sebagai dependency transitif.

"zpm remove" tidak menghapus source code project atau file yang dibuat oleh programmer.

92. Dependency Update

ZeroScript Package Manager ("zpm") menyediakan operasi untuk memperbarui dependency project berdasarkan version selector yang terdapat pada "project.zson".

Basic command:

zpm update

Perintah tersebut memeriksa dependency project dan mencari versi terbaru yang masih memenuhi selector yang ditentukan.

Contoh:

dependencies: {
    "http-utils": "1.2.x",
    "database": "2.x"
}

Jika tersedia:

http-utils:
1.2.0
1.2.5
1.2.10

database:
2.0.0
2.1.0
2.2.0

Maka "zpm update" dapat memilih:

http-utils@1.2.10
database@2.2.0

Aturan

- "zpm update" menggunakan version selector dari "project.zson".
- "zpm update" dapat mengganti versi yang tercatat pada "project.lock.zson".
- Versi baru harus tetap memenuhi version selector.
- Jika tidak tersedia versi yang lebih baru, dependency tetap menggunakan versi yang sedang digunakan.
- Package yang tidak memenuhi selector tidak boleh dipilih.
- Setelah update berhasil, "project.lock.zson" diperbarui dengan versi baru.
- Dependency transitif juga dapat diperbarui apabila versi barunya memenuhi seluruh kebutuhan dependency tree.
- Jika update menghasilkan konflik yang tidak dapat diselesaikan → ERROR.
- "zpm install" tidak melakukan upgrade otomatis terhadap versi yang sudah terkunci pada lockfile.
- "zpm update" adalah operasi eksplisit untuk meminta pembaruan dependency.

Contoh:

project.zson
    ↓
version selectors
    ↓
zpm update
    ↓
resolve newer versions
    ↓
update .zsc_m/
    ↓
update project.lock.zson

Dengan demikian, "zpm install" berfungsi untuk memasang dependency sesuai konfigurasi dan lockfile, sedangkan "zpm update" digunakan ketika programmer secara eksplisit ingin memperbarui dependency.

93. Git Package Source

ZeroScript Package Manager ("zpm") dapat menggunakan Git repository sebagai sumber package.

Git package source memungkinkan package didistribusikan tanpa memerlukan package server atau database khusus.

Basic command:

zpm install git:https://github.com/user/http-utils.git

Package kemudian disimpan pada:

.zsc_m/

Aturan

- Git package source menggunakan repository Git sebagai sumber package.
- URL repository harus menggunakan prefix "git:".
- Repository harus berisi package ZeroScript yang valid.
- "zpm" mengambil package dari repository dan menyimpannya ke ".zsc_m/".
- Package yang diambil melalui Git tetap mengikuti dependency dan module rules ZeroScript.
- Git package dapat menggunakan branch, tag, atau commit tertentu.
- Commit yang digunakan dapat dicatat pada "project.lock.zson".
- Jika source Git tidak dapat diakses atau repository tidak valid → ERROR.
- Git package tidak otomatis menjadi package global.
- ".zsc_m/" tetap dikelola oleh "zpm".

Contoh menggunakan tag:

zpm install git:https://github.com/user/http-utils.git#v1.2.0

Contoh menggunakan branch:

zpm install git:https://github.com/user/http-utils.git#main

Contoh menggunakan commit:

zpm install git:https://github.com/user/http-utils.git#8f42c1a

Untuk dependency yang ditulis dalam "project.zson", Git source dapat digunakan sebagai selector dependency:

{
    name: "my-app",
    version: "1.0.0",

    dependencies: {
        "http-utils": "git:https://github.com/user/http-utils.git#v1.2.0"
    }
}

Setelah dependency berhasil di-resolve, "project.lock.zson" dapat menyimpan referensi Git beserta commit konkret yang digunakan.

Git merupakan salah satu package source untuk "zpm", bukan bagian dari bahasa pemrograman ZeroScript.

94. Package Publishing

ZeroScript package dapat didistribusikan melalui Git repository.

Package publisher tidak memerlukan server package khusus. Sebuah Git repository dapat berfungsi sebagai source distribusi package.

Contoh struktur repository:

http-utils/
├── package.zson
├── client.zs
├── parser.zs
└── README.md

Package metadata didefinisikan dalam "package.zson".

Contoh:

{
    name: "http-utils",
    version: "1.2.0",
    description: "HTTP utilities for ZeroScript",

    dependencies: {}
}

Aturan

- Setiap package yang dapat dipublikasikan harus memiliki "package.zson".
- "name" merupakan nama package.
- "version" merupakan versi package menggunakan Semantic Versioning.
- "description" bersifat opsional.
- "dependencies" berisi dependency yang diperlukan package.
- Source code package menggunakan module system ZeroScript.
- Package dapat dipublikasikan melalui Git repository.
- Setiap versi package harus dapat diidentifikasi secara jelas.
- Tag Git dapat digunakan untuk menandai versi package.
- Contoh tag versi:
  v1.0.0
v1.1.0
v2.0.0
- "zpm" dapat menggunakan tag tersebut sebagai referensi versi package.
- Repository yang tidak memiliki "package.zson" yang valid → ERROR ketika digunakan sebagai package.
- Nama package pada "package.zson" harus sesuai dengan nama package yang digunakan oleh dependency resolver.
- Versi package tidak boleh diubah setelah versi tersebut dipublikasikan sebagai release yang telah digunakan.

Contoh penggunaan package yang dipublikasikan:

zpm install git:https://github.com/user/http-utils.git#v1.2.0

Package manager mengambil repository tersebut, membaca "package.zson", memvalidasi package, kemudian memasukkannya ke dependency environment project.

Sistem publishing berbasis Git merupakan mekanisme distribusi package awal ZeroScript. Registry khusus dapat ditambahkan kemudian tanpa mengubah struktur dasar package.

95. Package Registry

ZeroScript Package Manager ("zpm") dapat menggunakan package registry sebagai sumber pencarian dan distribusi package.

Registry berfungsi menyediakan informasi mengenai package yang tersedia, versi package, dan lokasi source package.

Registry awal ZeroScript dapat menggunakan Git repository sebagai backend distribusi tanpa memerlukan database atau server khusus.

Contoh:

ZeroScript Registry
├── http-utils
│   ├── 1.0.0
│   ├── 1.1.0
│   └── 1.2.0
├── database
│   ├── 1.0.0
│   └── 2.0.0
└── json-tools
    └── 1.0.0

Registry Index

Registry dapat memiliki index package yang berisi informasi seperti:

{
    name: "http-utils",
    versions: {
        "1.0.0": "https://github.com/user/http-utils.git#v1.0.0",
        "1.1.0": "https://github.com/user/http-utils.git#v1.1.0",
        "1.2.0": "https://github.com/user/http-utils.git#v1.2.0"
    }
}

Index tersebut dapat disimpan sebagai file dalam Git repository.

Aturan

- Registry berisi daftar package yang tersedia.
- Setiap package dapat memiliki beberapa versi.
- Setiap versi harus menunjuk ke source package yang valid.
- Registry harus menyediakan informasi yang cukup bagi "zpm" untuk melakukan version resolution.
- "zpm" menggunakan version selector dari "project.zson" untuk memilih versi.
- Hasil pemilihan versi dicatat pada "project.lock.zson".
- Registry tidak menyimpan source code package jika source package berada pada repository terpisah.
- Registry dapat menunjuk langsung ke Git repository package.
- Registry dapat menggunakan Git sebagai media distribusi index.
- Registry tidak wajib memiliki database.
- Registry tidak wajib memiliki backend HTTP khusus.
- Jika registry tidak dapat diakses atau index tidak valid → ERROR.
- Package yang tidak terdaftar pada registry tetap dapat digunakan melalui explicit Git source.
- Registry resmi dan registry tambahan dapat digunakan oleh "zpm".

Contoh:

zpm install http-utils

"zpm" mencari "http-utils" pada registry, menentukan versi berdasarkan selector dependency, kemudian mengambil source package dari lokasi yang diberikan registry.

Sedangkan:

zpm install git:https://github.com/user/http-utils.git#v1.2.0

langsung menggunakan Git source tanpa memerlukan package tersebut terdaftar pada registry.

Registry merupakan lapisan discovery dan metadata package. Sistem module, dependency, versioning, lockfile, dan storage tetap mengikuti aturan ZeroScript yang telah ditentukan sebelumnya.

96. Package Metadata

Setiap package ZeroScript memiliki metadata yang didefinisikan dalam "package.zson".

Metadata digunakan oleh "zpm", registry, dan developer untuk mengidentifikasi serta menjelaskan package.

Contoh:

{
    name: "http-utils",
    version: "1.2.0",
    description: "HTTP utilities for ZeroScript",
    author: "KaelTzy",
    license: "MIT",
    repository: "https://github.com/user/http-utils",

    dependencies: {}
}

Metadata Utama

- "name" — nama package.
- "version" — versi package menggunakan Semantic Versioning.
- "description" — deskripsi package.
- "author" — pembuat atau maintainer package.
- "license" — lisensi package.
- "repository" — lokasi source repository package.
- "dependencies" — dependency yang diperlukan package.

"name" dan "version" merupakan metadata wajib.

Metadata lainnya bersifat opsional kecuali ditentukan oleh registry yang digunakan.

Aturan

- "package.zson" harus berupa ZSON yang valid.
- "name" harus berupa string.
- "version" harus berupa string versi SemVer yang valid.
- "dependencies" harus berupa object jika tersedia.
- Nama package harus mengikuti aturan penamaan package ZeroScript.
- Metadata tidak dapat digunakan sebagai source code ZeroScript.
- "package.zson" tidak dapat di-import sebagai module ZeroScript.
- Metadata package dapat dibaca oleh "zpm" sebelum package digunakan.
- Metadata yang tidak valid → ERROR.
- Registry dapat menggunakan metadata package untuk menampilkan informasi package.
- Field tambahan dapat ditambahkan pada masa depan tanpa mengubah metadata inti.

Contoh struktur package:

http-utils/
├── package.zson
├── client.zs
├── parser.zs
└── README.md

"package.zson" mendeskripsikan package, sedangkan file ".zs" berisi source code package.

Metadata package tidak menentukan perilaku runtime package. Perilaku package ditentukan oleh source code ZeroScript dan dependency yang digunakan.

97. Package Integrity

ZeroScript Package Manager ("zpm") dapat melakukan verifikasi integritas package untuk memastikan package yang diterima sesuai dengan source yang telah ditentukan.

Verifikasi menggunakan hash dari package atau file package.

Contoh metadata registry:

{
    name: "http-utils",
    version: "1.2.0",
    source: "https://github.com/user/http-utils.git#v1.2.0",
    hash: "sha256:..."
}

Aturan

- Registry dapat menyediakan hash untuk setiap versi package.
- Hash digunakan untuk memverifikasi isi package setelah proses download.
- "zpm" menghitung hash package yang diterima dan membandingkannya dengan hash yang tercatat.
- Hash yang berbeda → ERROR.
- Package yang gagal verifikasi tidak boleh dianggap sebagai dependency yang berhasil di-install.
- Hash yang sudah tercatat pada lockfile harus digunakan ketika lockfile memerlukan verifikasi integritas.
- Git commit dapat digunakan sebagai identitas source ketika package menggunakan Git.
- Perubahan isi repository tanpa perubahan commit menghasilkan source yang tidak sesuai dengan commit yang tercatat dan dapat menyebabkan verifikasi gagal.
- Verifikasi integritas dilakukan sebelum package digunakan oleh project.
- Integritas package tidak menggantikan pemeriksaan keamanan terhadap source code package.

Contoh lockfile:

{
    packages: {
        "http-utils": {
            version: "1.2.0",
            source: "https://github.com/user/http-utils.git",
            commit: "8f42c1a",
            hash: "sha256:..."
        }
    }
}

Dengan demikian, "project.lock.zson" tidak hanya dapat mencatat versi package, tetapi juga source dan identitas package yang digunakan.

Package integrity bertujuan menjaga reproducibility dan keutuhan dependency, bukan menentukan apakah source code package aman atau terpercaya.

98. Package Cache

ZeroScript Package Manager ("zpm") dapat menyimpan package yang telah di-download ke dalam local cache.

Cache digunakan untuk mengurangi download berulang ketika package yang sama diperlukan kembali.

Contoh struktur:

~/.zsc_cache/
├── http-utils/
│   └── 1.2.0/
├── database/
│   └── 2.0.0/
└── ...

Cache berbeda dengan ".zsc_m/".

- ".zsc_m/" berisi dependency yang digunakan oleh project.
- ".zsc_cache/" berisi salinan package yang dapat digunakan kembali oleh "zpm".

Aturan

- "zpm" dapat menggunakan package dari cache jika versi dan identitas package sesuai dengan kebutuhan.
- Package dari cache tetap harus melalui verifikasi integritas.
- Cache yang tidak valid atau rusak tidak boleh digunakan.
- Cache yang tidak sesuai dengan lockfile tidak boleh menggantikan dependency yang dikunci.
- Jika package tidak tersedia di cache, "zpm" mengambilnya dari source atau registry.
- Setelah berhasil diambil dan diverifikasi, package dapat disimpan ke cache.
- Menghapus cache tidak menghapus dependency yang sedang digunakan oleh project.
- Cache dapat dibersihkan tanpa mengubah "project.zson" atau "project.lock.zson".
- Cache dikelola oleh "zpm".

Contoh alur:

zpm install
    ↓
check .zsc_cache/
    ↓
package tersedia dan valid?
    ├── YA  → gunakan cache
    └── TIDAK
          ↓
       download
          ↓
       verify
          ↓
       simpan cache
          ↓
       install ke .zsc_m/

Package cache merupakan optimasi lokal dan tidak mengubah dependency resolution.

Project tetap menggunakan ".zsc_m/" sebagai dependency environment-nya.

99. Package Source Priority

ZeroScript Package Manager ("zpm") dapat memperoleh package dari beberapa source, seperti registry, Git repository, dan local cache.

Agar proses resolution tetap deterministik, "zpm" menggunakan urutan prioritas source yang telah ditentukan.

Urutan default:

Local Cache
    ↓
Registry
    ↓
Explicit Git Source

Namun, explicit source yang ditulis programmer memiliki prioritas langsung terhadap registry.

Contoh:

zpm install git:https://github.com/user/http-utils.git#v1.2.0

Perintah tersebut langsung menggunakan Git source yang diberikan dan tidak mencari package tersebut melalui registry.

Aturan

- Local cache dapat digunakan apabila package yang sesuai tersedia dan lolos verifikasi.
- Package biasa seperti:
  zpm install http-utils
  menggunakan registry untuk menemukan package.
- Explicit Git source seperti:
  zpm install git:https://github.com/user/http-utils.git#v1.2.0
  menggunakan source Git yang diberikan.
- Source yang tidak sesuai dengan dependency requirement → ERROR.
- Source yang gagal diakses → ERROR jika tidak terdapat source alternatif yang valid.
- "zpm" tidak boleh memilih source secara acak.
- Source priority tidak boleh menyebabkan dependency menggunakan versi yang tidak memenuhi selector.
- Lockfile memiliki prioritas terhadap proses resolution ulang selama source dan package yang dikunci masih valid.
- Source priority berlaku untuk proses installation dan dependency resolution, bukan untuk module scope ZeroScript.

Contoh:

project.lock.zson
       ↓
locked source/version valid?
       ├── YA → gunakan
       └── TIDAK
             ↓
          resolve source
             ↓
          cache / registry / explicit source

Aturan source priority dibuat agar "zpm" tetap predictable, terutama ketika package tersedia melalui lebih dari satu sumber.

100. Package System Completion

ZeroScript Package Manager ("zpm") menggunakan sistem package dan dependency yang terdiri dari project manifest, version resolution, lockfile, installation, Git source, registry, cache, dan integrity verification.

Sistem package yang telah ditentukan mencakup:

project.zson
    ↓
dependencies
    ↓
version resolution
    ↓
project.lock.zson
    ↓
package source
    ↓
integrity verification
    ↓
.zsc_cache/
    ↓
.zsc_m/
    ↓
ZeroScript module system

Komponen Utama

- "project.zson" — konfigurasi project dan dependency selector.
- "project.lock.zson" — versi dan source dependency yang telah di-resolve.
- "package.zson" — metadata package.
- ".zsc_m/" — dependency environment project.
- ".zsc_cache/" — cache package lokal.
- "zpm" — package manager.
- Git — salah satu source package.
- Registry — discovery dan metadata package.
- Hash — verifikasi integritas package.

Command Utama

zpm install
zpm remove <package>
zpm update
zpm run <script>

Source Git dapat digunakan secara langsung:

zpm install git:https://github.com/user/package.git#v1.0.0

Aturan Umum

- Dependency harus dideklarasikan secara eksplisit.
- Version selector harus mengikuti aturan versioning ZeroScript.
- Lockfile digunakan untuk reproducible installation.
- Dependency transitif memiliki scope sendiri.
- Konflik dependency harus diselesaikan secara deterministik.
- Package yang gagal validasi atau integrity verification tidak boleh digunakan.
- Package environment project terisolasi dari project lain.
- Package manager tidak mengubah source code project.
- Registry dan Git hanya menjadi sumber distribusi; keduanya tidak mengubah aturan module system ZeroScript.
- Package system tidak menjadi bagian dari syntax bahasa ZeroScript.

Dengan Section 100, core package dan dependency system ZeroScript dianggap selesai untuk spesifikasi tahap ini.

Fitur tambahan seperti private registry, authentication, package signing, workspace/monorepo, dan advanced publishing dapat ditambahkan sebagai extension pada masa depan tanpa mengubah core package system.
