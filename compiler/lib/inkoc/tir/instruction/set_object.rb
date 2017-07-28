# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetObject
        include Inspect

        attr_reader :register, :global, :prototype, :location

        def initialize(register, global, prototype, location)
          @register = register
          @global = global
          @prototype = prototype
          @location = location
        end
      end
    end
  end
end
