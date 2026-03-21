require "minitest/autorun"
require "tempfile"
require "wcl"

if ENV["CI_REPORTS"]
  require "minitest/reporters"
  Minitest::Reporters.use! [
    Minitest::Reporters::DefaultReporter.new,
    Minitest::Reporters::JUnitReporter.new(ENV["CI_REPORTS"])
  ]
end

module WclTestHelper
  def create_tmp_wcl(content, name: "test.wcl")
    dir = Dir.mktmpdir
    path = File.join(dir, name)
    File.write(path, content)
    path
  end
end
