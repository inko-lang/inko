# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Try
        include Inspect

        attr_reader :register, :try_body, :else_body, :else_arg, :location

        def initialize(register, try_body, else_body, else_arg, location)
          @register = register
          @try_body = try_body
          @else_body = else_body
          @else_arg = else_arg
          @location = location
        end
      end
    end
  end
end
