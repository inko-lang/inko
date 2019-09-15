#!/usr/bin/env ruby
# frozen_string_literal: true
# rubocop: disable all

require 'json'
require 'open3'

stdout, _, _ = Open3.capture3('cargo audit -f vm/Cargo.lock --json')
json = stdout.strip

report = {
  version: '2.1',
  vulnerabilities: [],
  remediations: []
}

unless json.empty?
  raw_report = JSON.parse(json)

  raw_report['vulnerabilities']['list'].each do |vuln|
    advisory = vuln['advisory']

    report[:vulnerabilities] << {
      category: 'dependency_scanning',
      name: advisory['title'],
      message: advisory['title'],
      description: advisory['description'],
      cve: "Cargo.lock:#{advisory['package']}:#{advisory['id']}",
      severity: 'High',
      confidence: 'Confirmed',
      solution: "Upgrade to #{advisory['patched_versions'].join(', ')}",
      scanner: {
        id: 'rustsec',
        name: 'RustSec'
      },
      location: {
        file: raw_report['lockfile']['path'],
        dependency: {
          package: {
            name: advisory['package']
          },
          version: vuln['package']['version']
        }
      },
      identifiers: [
        {
          type: 'rustsec',
          name: advisory['id'],
          value: advisory['id'],
          url: "https://github.com/RustSec/advisory-db/blob/master/crates/#{advisory['package']}/#{advisory['id']}.toml"
        }
      ],
      links: [
        {
          name: 'RustSec advisory',
          url: "https://github.com/RustSec/advisory-db/blob/master/crates/#{advisory['package']}/#{advisory['id']}.toml"
        },
        {
          name: 'Issue',
          url: advisory['url']
        }
      ]
    }
  end
end

File.open('gl-dependency-scanning-report.json', 'w') do |handle|
  handle.write(JSON.pretty_generate(report))
end
