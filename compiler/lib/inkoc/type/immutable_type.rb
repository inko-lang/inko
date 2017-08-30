# frozen_string_literal: true

module Inkoc
  module Type
    module ImmutableType
      attr_reader :prototype

      def initialize(prototype)
        @prototype = prototype
      end

      def attributes
        prototype.attributes
      end

      def implemented_traits
        prototype.implemented_traits
      end
    end
  end
end
