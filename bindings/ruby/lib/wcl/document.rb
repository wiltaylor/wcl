require "json"

module Wcl
  # A parsed and evaluated WCL document.
  class Document
    def initialize(handle)
      @handle = handle
      @closed = false
      @values = nil
      @diagnostics = nil

      ObjectSpace.define_finalizer(self, self.class._invoke_release(handle))
    end

    def self._invoke_release(handle)
      proc { WasmRuntime.get.document_free(handle) rescue nil }
    end

    def values
      raise "Document is closed" if @closed

      @values ||= Convert.json_to_values(WasmRuntime.get.document_values(@handle))
    end

    def has_errors?
      raise "Document is closed" if @closed

      WasmRuntime.get.document_has_errors(@handle)
    end

    def errors
      diagnostics.select(&:error?)
    end

    def diagnostics
      raise "Document is closed" if @closed

      @diagnostics ||= Convert.json_to_diagnostics(WasmRuntime.get.document_diagnostics(@handle))
    end

    def query(query_str)
      raise "Document is closed" if @closed

      json_str = WasmRuntime.get.document_query(@handle, query_str)
      result = JSON.parse(json_str)
      raise ValueError, result["error"] if result.key?("error")

      Convert.json_to_ruby(result["ok"])
    end

    def blocks
      raise "Document is closed" if @closed

      json_str = WasmRuntime.get.document_blocks(@handle)
      Convert.json_to_blocks(json_str)
    end

    def blocks_of_type(kind)
      raise "Document is closed" if @closed

      json_str = WasmRuntime.get.document_blocks_of_type(@handle, kind)
      Convert.json_to_blocks(json_str)
    end

    def close
      return if @closed

      @closed = true
      WasmRuntime.get.document_free(@handle)
      ObjectSpace.undefine_finalizer(self)
    end

    def to_h
      { values: values, has_errors: has_errors?, diagnostics: diagnostics.map(&:inspect) }
    end
  end

  # Error raised for invalid queries.
  class ValueError < StandardError; end
end
