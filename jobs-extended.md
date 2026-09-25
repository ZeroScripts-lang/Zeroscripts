Berdasarkan audit Fase 1-3 (6 gap yang sudah ditemukan), susun RENCANA perbaikan
tertulis. JANGAN eksekusi/ubah kode apapun dulu.

Rencana harus mencakup, secara spesifik dan dengan nomor baris:

1. Perubahan struct: Value::Closure.env dari Rc<RefCell<HashMap<String, Value>>>
   menjadi HeapRef. Frame.captured_env dan Frame.scope_env dari
   Option<Rc<RefCell<HashMap<String, Value>>>> menjadi Option<HeapRef>.

2. execute_make_closure (vm.rs:1150-1208): ganti Rc::new(RefCell::new(new_env))
   menjadi alokasi lewat self.heap.allocate(HeapObject::Environment(...)) yang
   sudah ada strukturnya di heap.rs:36-41.

3. execute_load_captured & execute_store_captured: ganti akses .borrow()/
   .borrow_mut() ke Rc, menjadi akses lewat HeapRef ke HeapObject::Environment
   di heap. Jelaskan bagaimana caranya baca/tulis bindings di
   HeapObject::Environment(Vec<(String, HeapRef)>) — apakah perlu helper method
   baru di heap.rs untuk lookup/update by name.

4. Verifikasi bahwa gc.rs sudah bisa trace HeapRef closure/environment ini
   lewat child_refs (heap.rs:229-237) sebagai root reachable dari value stack
   dan frame locals — sebutkan apakah ada bagian gc.rs yang perlu ditambah.

5. Bagaimana shared environment semantics (kasus "dua closure share env yang
   sama") tetap terjaga tanpa Rc — apakah cukup dengan menyimpan HeapRef yang
   sama (karena HeapRef itu sendiri sudah merupakan cara membagikan reference
   ke object heap yang sama, mirip cara list/object dibagikan sesuai spec).

6. Dampak terhadap 4 test closure (test_closure_arrow_function,
   test_closure_mutation_shared, test_closure_nested,
   test_closure_multiple_closures_same_env) — apakah tetap pass dengan
   pendekatan HeapRef ini.

7. File dan baris lain yang mungkin ikut terdampak (compiler.rs, to_runtime_value,
   from_heap_ref, dll).

Tulis rencana ini secara lengkap. Saya akan review dulu. JANGAN mulai menulis
atau mengubah kode sampai saya bilang "lanjut eksekusi".
