# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class GetAttribute
        include Inspect

        attr_reader :register, :receiver, :name, :location

        def initialize(register, receiver, name, location)
          @register = register
          @receiver = receiver
          @name = name
          @location = location
        end
      end
    end
  end
end
