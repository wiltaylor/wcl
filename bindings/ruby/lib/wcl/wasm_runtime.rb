require "wasmtime"

module Wcl
  # Singleton WASM runtime that loads and manages the wcl_wasm module.
  class WasmRuntime
    @instance = nil
    @init_mutex = Mutex.new

    def self.get
      return @instance if @instance

      @init_mutex.synchronize do
        @instance ||= new
      end
      @instance
    end

    def initialize
      @mutex = Mutex.new
      @engine = Wasmtime::Engine.new

      wasm_path = File.join(__dir__, "wcl_wasm.wasm")
      @module = Wasmtime::Module.from_file(@engine, wasm_path)

      @linker = Wasmtime::Linker.new(@engine)
      Wasmtime::WASI::P1.add_to_linker_sync(@linker)
      define_host_functions

      @store = Wasmtime::Store.new(@engine, wasi_p1_config: Wasmtime::WasiConfig.new)
      @wasm_instance = @linker.instantiate(@store, @module)

      # Cache exported functions
      @alloc = @wasm_instance.export("wcl_wasm_alloc").to_func
      @dealloc = @wasm_instance.export("wcl_wasm_dealloc").to_func
      @string_free = @wasm_instance.export("wcl_wasm_string_free").to_func
      @parse_fn = @wasm_instance.export("wcl_wasm_parse").to_func
      @parse_with_functions_fn = @wasm_instance.export("wcl_wasm_parse_with_functions").to_func
      @doc_free = @wasm_instance.export("wcl_wasm_document_free").to_func
      @doc_values = @wasm_instance.export("wcl_wasm_document_values").to_func
      @doc_has_errors = @wasm_instance.export("wcl_wasm_document_has_errors").to_func
      @doc_diagnostics = @wasm_instance.export("wcl_wasm_document_diagnostics").to_func
      @doc_query = @wasm_instance.export("wcl_wasm_document_query").to_func
      @doc_blocks = @wasm_instance.export("wcl_wasm_document_blocks").to_func
      @doc_blocks_of_type = @wasm_instance.export("wcl_wasm_document_blocks_of_type").to_func
      @memory = @wasm_instance.export("memory").to_memory
    end

    # -- Public API (all synchronized) ------------------------------------

    def parse(source, options_json)
      @mutex.synchronize do
        src_ptr = write_string(source)
        opts_ptr = write_string(options_json)
        begin
          @parse_fn.call(src_ptr, opts_ptr)
        ensure
          dealloc_string(src_ptr, source) if src_ptr != 0
          dealloc_string(opts_ptr, options_json) if opts_ptr != 0 && options_json
        end
      end
    end

    def parse_with_functions(source, options_json, func_names_json)
      @mutex.synchronize do
        src_ptr = write_string(source)
        opts_ptr = write_string(options_json)
        names_ptr = write_string(func_names_json)
        begin
          @parse_with_functions_fn.call(src_ptr, opts_ptr, names_ptr)
        ensure
          dealloc_string(src_ptr, source) if src_ptr != 0
          dealloc_string(opts_ptr, options_json) if opts_ptr != 0 && options_json
          dealloc_string(names_ptr, func_names_json) if names_ptr != 0
        end
      end
    end

    def document_free(handle)
      @mutex.synchronize { @doc_free.call(handle) }
    end

    def document_values(handle)
      @mutex.synchronize do
        ptr = @doc_values.call(handle)
        consume_string(ptr)
      end
    end

    def document_has_errors(handle)
      @mutex.synchronize { @doc_has_errors.call(handle) != 0 }
    end

    def document_diagnostics(handle)
      @mutex.synchronize do
        ptr = @doc_diagnostics.call(handle)
        consume_string(ptr)
      end
    end

    def document_query(handle, query)
      @mutex.synchronize do
        q_ptr = write_string(query)
        begin
          ptr = @doc_query.call(handle, q_ptr)
          consume_string(ptr)
        ensure
          dealloc_string(q_ptr, query) if q_ptr != 0
        end
      end
    end

    def document_blocks(handle)
      @mutex.synchronize do
        ptr = @doc_blocks.call(handle)
        consume_string(ptr)
      end
    end

    def document_blocks_of_type(handle, kind)
      @mutex.synchronize do
        k_ptr = write_string(kind)
        begin
          ptr = @doc_blocks_of_type.call(handle, k_ptr)
          consume_string(ptr)
        ensure
          dealloc_string(k_ptr, kind) if k_ptr != 0
        end
      end
    end

    private

    def define_host_functions
      @linker.func_new(
        "env", "host_call_function",
        [:i32, :i32, :i32, :i32, :i32, :i32], [:i32]
      ) do |caller, name_ptr, name_len, args_ptr, args_len, result_ptr_out, result_len_out|
        memory = caller.export("memory").to_memory

        name = memory.read(name_ptr, name_len).force_encoding("UTF-8")
        args_json = memory.read(args_ptr, args_len).force_encoding("UTF-8")

        success, result_json = Callback.invoke(name, args_json)

        if result_json
          result_bytes = result_json.encode("UTF-8")
          alloc_fn = caller.export("wcl_wasm_alloc").to_func
          ptr = alloc_fn.call(result_bytes.bytesize)

          memory.write(ptr, result_bytes)
          memory.write(result_ptr_out, [ptr].pack("V"))
          memory.write(result_len_out, [result_bytes.bytesize].pack("V"))
        end

        success ? 0 : -1
      end
    end

    def write_string(str)
      return 0 if str.nil?

      encoded = str.encode("UTF-8")
      ptr = @alloc.call(encoded.bytesize + 1)
      @memory.write(ptr, encoded + "\0")
      ptr
    end

    def read_c_string(ptr)
      return "" if ptr == 0

      data = @memory.read(ptr, @memory.data_size - ptr)
      null_idx = data.index("\0") || data.size
      data[0, null_idx].force_encoding("UTF-8")
    end

    def consume_string(ptr)
      return "" if ptr == 0

      s = read_c_string(ptr)
      @string_free.call(ptr)
      s
    end

    def dealloc_string(ptr, original)
      @dealloc.call(ptr, original.encode("UTF-8").bytesize + 1)
    end
  end
end
