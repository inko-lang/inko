rule '.rb' => '.rll' do |task|
  sh "ruby-ll #{task.source} -o #{task.name}"
end

desc 'Generates the parser'
task :parser => ['lib/inko/parser.rb']
