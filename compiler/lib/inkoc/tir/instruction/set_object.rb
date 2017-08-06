# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetObject
        include Inspect

        attr_reader :register, :prototype, :location

        def initialize(register, permanent, prototype, location)
          @register = register
          @permanent = permanent
          @prototype = prototype
          @location = location
        end

        def permanent?
          @permanent
        end
      end
    end
  end
end
