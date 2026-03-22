# WCL Ruby binding example.

require "wcl"

# Read the shared config file
config_path = File.join(__dir__, "..", "config", "app.wcl")
source = File.read(config_path)

# Parse the source
doc = Wcl.parse(source)

# Check for errors
if doc.has_errors?
  puts "Parse errors:"
  doc.errors.each { |err| puts "  - #{err.message}" }
  exit 1
end

puts "Parsed successfully!"

# Count server blocks
servers = doc.blocks_of_type("server")
puts "Server blocks: #{servers.length}"

# Print server names and ports
puts "\nServers:"
server_values = doc.values["server"]
if server_values.is_a?(Hash)
  server_values.each do |name, attrs|
    port = attrs.is_a?(Hash) ? attrs["port"] : "?"
    puts "  #{name}: port #{port}"
  end
end

# Query for servers with workers > 2
puts "\nQuery: server | .workers > 2"
begin
  result = doc.query("server | .workers > 2")
  puts "  Result: #{result.inspect}"
rescue => e
  puts "  Query error: #{e.message}"
end

# Print the users table
puts "\nUsers table:"
users = doc.values["users"]
if users.is_a?(Array)
  users.each do |row|
    if row.is_a?(Hash)
      puts "  #{row['name']} | #{row['role']} | admin=#{row['admin']}"
    end
  end
end
