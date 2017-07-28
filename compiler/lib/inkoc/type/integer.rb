# frozen_string_literal: true

module Inkoc
  module Type
    class Integer
      include Inspect
      include ObjectOperations
      include TypeCompatibility

      attr_reader :prototype

      def initialize(prototype)
        @prototype = prototype
      end

      def attributes
        @prototype.attributes
      end

      def block?
        false
      end
    end
  end
end
