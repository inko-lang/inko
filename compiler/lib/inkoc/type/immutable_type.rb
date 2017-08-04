# frozen_string_literal: true

module Inkoc
  module Type
    module ImmutableType
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
