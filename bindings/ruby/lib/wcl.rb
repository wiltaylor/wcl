require "json"
require_relative "wcl/version"
require_relative "wcl/types"
require_relative "wcl/callback"
require_relative "wcl/convert"
require_relative "wcl/wasm_runtime"
require_relative "wcl/document"

module Wcl
  module_function

  # Parse a WCL source string and return a Document.
  def parse(source, root_dir: nil, allow_imports: nil, max_import_depth: nil,
            max_macro_depth: nil, max_loop_depth: nil, max_iterations: nil,
            functions: nil, variables: nil)
    options = {}
    options["rootDir"] = root_dir.to_s if root_dir
    options["allowImports"] = allow_imports unless allow_imports.nil?
    options["maxImportDepth"] = max_import_depth if max_import_depth
    options["maxMacroDepth"] = max_macro_depth if max_macro_depth
    options["maxLoopDepth"] = max_loop_depth if max_loop_depth
    options["maxIterations"] = max_iterations if max_iterations
    options["variables"] = variables if variables

    options_json = options.empty? ? nil : JSON.generate(options)
    runtime = WasmRuntime.get

    if functions && !functions.empty?
      Callback.set_functions(functions)
      begin
        func_names_json = JSON.generate(functions.keys)
        handle = runtime.parse_with_functions(source, options_json, func_names_json)
      ensure
        Callback.clear_functions
      end
    else
      handle = runtime.parse(source, options_json)
    end

    Document.new(handle)
  end

  # Parse a WCL file and return a Document.
  def parse_file(path, **kwargs)
    path = path.to_s
    source = File.read(path)
  rescue Errno::ENOENT, Errno::EACCES => e
    raise IOError, "#{path}: #{e.message}"
  else
    kwargs[:root_dir] ||= File.dirname(File.expand_path(path))
    parse(source, **kwargs)
  end
end
