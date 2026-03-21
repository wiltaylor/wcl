require "json"

module Wcl
  # Thread-local callback bridge for custom functions invoked from WASM.
  module Callback
    module_function

    def set_functions(fn_hash)
      Thread.current[:wcl_functions] = fn_hash
    end

    def clear_functions
      Thread.current[:wcl_functions] = nil
    end

    def invoke(name, args_json)
      functions = Thread.current[:wcl_functions]
      return [false, "callback not found: #{name}"] if functions.nil? || !functions.key?(name)

      begin
        args = JSON.parse(args_json)
        ruby_args = args.map { |a| json_to_ruby(a) }
        result = functions[name].call(ruby_args)
        result_json = JSON.generate(ruby_to_json(result))
        [true, result_json]
      rescue => e
        [false, e.message]
      end
    end

    def json_to_ruby(val)
      case val
      when nil, true, false, Integer, Float, String
        val
      when Array
        val.map { |v| json_to_ruby(v) }
      when Hash
        val.transform_values { |v| json_to_ruby(v) }
      else
        val
      end
    end

    def ruby_to_json(val)
      case val
      when nil, true, false, Integer, Float, String
        val
      when Array
        val.map { |v| ruby_to_json(v) }
      when Hash
        val.transform_values { |v| ruby_to_json(v) }
      when Set
        { "__type" => "set", "items" => val.map { |v| ruby_to_json(v) } }
      else
        val
      end
    end
  end
end
