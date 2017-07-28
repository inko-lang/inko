# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetBlock
        include Inspect

        attr_reader :register, :body, :location

        def initialize(register, body, location)
          @register = register
          @body = body
          @location = location
        end
      end
    end
  end
end
