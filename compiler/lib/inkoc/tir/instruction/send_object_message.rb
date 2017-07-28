# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SendObjectMessage
        include Inspect

        attr_reader :register, :receiver, :name, :arguments, :location

        def initialize(register, receiver, name, arguments, location)
          @register = register
          @receiver = receiver
          @name = name
          @arguments = arguments
          @location = location
        end
      end
    end
  end
end
