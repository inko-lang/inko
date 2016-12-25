rule '.rb' => '.rl' do |task|
  sh "ragel -F1 -R #{task.source} -o #{task.name}"
end

desc 'Generates the lexer'
task :lexer => ['lib/inko/lexer.rb']
