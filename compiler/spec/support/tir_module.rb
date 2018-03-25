# frozen_string_literal: true

module Support
  module TirModule
    def new_tir_module
      loc = Inkoc::SourceLocation.first_line(Inkoc::SourceFile.new('rspec'))
      name = Inkoc::TIR::QualifiedName.new(%w[rspec])

      Inkoc::TIR::Module.new(name, loc)
    end
  end
end
