# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class LoadModule
        include Inspect

        attr_reader :register, :path, :location

        def initialize(register, path, location)
          @register = register
          @path = path
          @location = location
        end
      end
    end
  end
end
