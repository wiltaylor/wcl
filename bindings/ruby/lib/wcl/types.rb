module Wcl
  # A reference to a WCL block with its attributes.
  class BlockRef
    attr_reader :kind, :id, :attributes, :children, :decorators

    def initialize(kind:, id: nil, attributes: {}, children: [], decorators: [])
      @kind = kind
      @id = id
      @attributes = attributes
      @children = children
      @decorators = decorators
    end

    def get(key)
      @attributes[key]
    end

    def [](key)
      @attributes[key]
    end

    def has_decorator?(name)
      @decorators.any? { |d| d.name == name }
    end

    def to_h
      {
        kind: @kind,
        id: @id,
        attributes: @attributes,
        children: @children.map(&:to_h),
        decorators: @decorators.map(&:to_h)
      }
    end

    def inspect
      if @id
        "#<Wcl::BlockRef(#{@kind} #{@id})>"
      else
        "#<Wcl::BlockRef(#{@kind})>"
      end
    end
    alias_method :to_s, :inspect
  end

  # A WCL decorator with name and arguments.
  class Decorator
    attr_reader :name, :args

    def initialize(name:, args: {})
      @name = name
      @args = args
    end

    def to_h
      { name: @name, args: @args }
    end

    def inspect
      "#<Wcl::Decorator(@#{@name})>"
    end
    alias_method :to_s, :inspect
  end

  # A WCL diagnostic (error, warning, etc.).
  class Diagnostic
    attr_reader :severity, :message, :code

    def initialize(severity:, message:, code: nil)
      @severity = severity
      @message = message
      @code = code
    end

    def error?
      @severity == "error"
    end

    def warning?
      @severity == "warning"
    end

    def inspect
      if @code
        "#<Wcl::Diagnostic(#{@severity}: [#{@code}] #{@message})>"
      else
        "#<Wcl::Diagnostic(#{@severity}: #{@message})>"
      end
    end
    alias_method :to_s, :inspect
  end
end
