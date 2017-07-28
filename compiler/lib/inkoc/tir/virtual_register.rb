# frozen_string_literal: true

module Inkoc
  module TIR
    class VirtualRegister
      include Inspect

      attr_reader :id, :type

      def initialize(id, type)
        @id = id
        @type = type
      end
    end
  end
end
