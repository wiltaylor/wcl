require_relative "test_helper"

class TestSchema < Minitest::Test
  def test_missing_required_field
    source = <<~WCL
      schema "config" {
          port: i64
          host: string
      }

      config {
          port = 8080
      }
    WCL
    doc = Wcl.parse(source)
    error_codes = doc.errors.map(&:code)
    assert_includes error_codes, "E070", "expected E070 in #{error_codes}"
  end

  def test_type_mismatch
    source = <<~WCL
      schema "config" {
          port: i64
      }

      config {
          port = "not_a_number"
      }
    WCL
    doc = Wcl.parse(source)
    error_codes = doc.errors.map(&:code)
    assert_includes error_codes, "E071", "expected E071 in #{error_codes}"
  end

  def test_valid_schema_no_errors
    source = <<~WCL
      schema "config" {
          port: i64
          host: string
      }

      config {
          port = 8080
          host = "localhost"
      }
    WCL
    doc = Wcl.parse(source)
    schema_errors = doc.errors.select { |d| %w[E070 E071 E072].include?(d.code) }
    assert_equal 0, schema_errors.size, "unexpected schema errors: #{schema_errors.inspect}"
  end

  def test_closed_schema_unknown_field
    source = <<~WCL
      @closed
      schema "strict" {
          name: string
      }

      strict {
          name = "ok"
          extra = 123
      }
    WCL
    doc = Wcl.parse(source)
    error_codes = doc.errors.map(&:code)
    assert_includes error_codes, "E072", "expected E072 in #{error_codes}"
  end

  def test_constraint_violation
    source = <<~WCL
      schema "config" {
          port: i64 @validate(min=1, max=65535)
      }

      config {
          port = 99999
      }
    WCL
    doc = Wcl.parse(source)
    error_codes = doc.errors.map(&:code)
    assert_includes error_codes, "E073", "expected E073 in #{error_codes}"
  end

  def test_children_constraint_allows_valid
    source = <<~WCL
      @children(["endpoint"])
      schema "service" {}

      service main {
          endpoint health {}
      }
    WCL
    doc = Wcl.parse(source)
    containment_errors = doc.errors.select { |d| %w[E095 E096].include?(d.code) }
    assert_equal 0, containment_errors.size, "unexpected containment errors: #{containment_errors.inspect}"
  end

  def test_children_constraint_rejects_invalid
    source = <<~WCL
      @children(["endpoint"])
      schema "service" {}

      service main {
          middleware auth {}
      }
    WCL
    doc = Wcl.parse(source)
    error_codes = doc.errors.map(&:code)
    assert_includes error_codes, "E095", "expected E095 in #{error_codes}"
  end

  def test_parent_constraint_rejects_at_root
    source = <<~WCL
      @parent(["service"])
      schema "endpoint" {}

      endpoint orphan {}
    WCL
    doc = Wcl.parse(source)
    error_codes = doc.errors.map(&:code)
    assert_includes error_codes, "E096", "expected E096 in #{error_codes}"
  end

  def test_parent_constraint_allows_valid
    source = <<~WCL
      @parent(["service"])
      schema "endpoint" {}

      service main {
          endpoint health {}
      }
    WCL
    doc = Wcl.parse(source)
    containment_errors = doc.errors.select { |d| %w[E095 E096].include?(d.code) }
    assert_equal 0, containment_errors.size, "unexpected containment errors: #{containment_errors.inspect}"
  end
end
