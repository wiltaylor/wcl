require "json"

module Wcl
  # JSON-to-Ruby value conversion for WCL WASM binding.
  module Convert
    module_function

    def json_to_ruby(val)
      case val
      when nil, true, false, String
        val
      when Integer
        val
      when Float
        val == val.to_i && !val.is_a?(Float) ? val.to_i : val
      when Array
        val.map { |v| json_to_ruby(v) }
      when Hash
        # Check for set encoding
        if val["__type"] == "set" && val.key?("items")
          items = val["items"].map { |v| json_to_ruby(v) }
          begin
            Set.new(items)
          rescue
            items
          end
        # Check for block ref encoding
        elsif val.key?("kind") && (val.key?("attributes") || val.key?("children") || val.key?("decorators"))
          json_to_block_ref(val)
        else
          val.transform_values { |v| json_to_ruby(v) }
        end
      else
        val
      end
    end

    def json_to_values(json_str)
      data = JSON.parse(json_str)
      data.transform_values { |v| json_to_ruby(v) }
    end

    def json_to_blocks(json_str)
      data = JSON.parse(json_str)
      data.map { |b| json_to_block_ref(b) }
    end

    def json_to_diagnostics(json_str)
      data = JSON.parse(json_str)
      data.map do |d|
        Diagnostic.new(
          severity: d["severity"],
          message: d["message"],
          code: d["code"]
        )
      end
    end

    def json_to_block_ref(obj)
      attrs = (obj["attributes"] || {}).transform_values { |v| json_to_ruby(v) }
      children = (obj["children"] || []).map { |c| json_to_block_ref(c) }
      decorators = (obj["decorators"] || []).map do |d|
        Decorator.new(
          name: d["name"],
          args: (d["args"] || {}).transform_values { |v| json_to_ruby(v) }
        )
      end

      BlockRef.new(
        kind: obj["kind"],
        id: obj["id"],
        attributes: attrs,
        children: children,
        decorators: decorators
      )
    end
  end
end
