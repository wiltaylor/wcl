require "minitest/autorun"
require "tempfile"
require "wcl"

module WclTestHelper
  def create_tmp_wcl(content, name: "test.wcl")
    dir = Dir.mktmpdir
    path = File.join(dir, name)
    File.write(path, content)
    path
  end
end
