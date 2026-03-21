require_relative "test_helper"

class TestDiagnostics < Minitest::Test
  def test_severity_values
    doc = Wcl.parse("= invalid")
    assert doc.diagnostics.size > 0
    doc.diagnostics.each do |d|
      assert_includes %w[error warning info hint], d.severity
    end
  end

  def test_error_has_message
    doc = Wcl.parse("= = =")
    assert doc.has_errors?
    doc.errors.each do |e|
      assert_kind_of String, e.message
      assert e.message.size > 0
    end
  end

  def test_valid_doc_no_errors
    doc = Wcl.parse("x = 42")
    refute doc.has_errors?
    assert_equal 0, doc.errors.size
  end

  def test_diagnostic_code
    source = <<~WCL
      schema "cfg" { port: int }
      cfg { port = "bad" }
    WCL
    doc = Wcl.parse(source)
    coded = doc.diagnostics.select { |d| d.code }
    assert coded.size > 0
  end

  def test_diagnostic_inspect
    doc = Wcl.parse("= = =")
    doc.diagnostics.each do |d|
      assert_includes d.inspect, "Diagnostic("
    end
  end

  def test_errors_only_subset
    source = <<~WCL
      @warning
      validation "soft check" {
          let x = -1
          check = x > 0
          message = "x not positive"
      }
    WCL
    doc = Wcl.parse(source)
    doc.errors.each do |e|
      assert_equal "error", e.severity
    end
  end

  def test_diagnostics_include_warnings
    source = <<~WCL
      @warning
      validation "soft check" {
          let x = -1
          check = x > 0
          message = "x not positive"
      }
    WCL
    doc = Wcl.parse(source)
    warnings = doc.diagnostics.select(&:warning?)
    assert warnings.size > 0
  end
end
