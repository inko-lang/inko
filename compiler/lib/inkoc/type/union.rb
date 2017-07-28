# frozen_string_literal: true

module Inkoc
  module Type
    class Union
      include Inspect

      # The types that are unioned together.
      attr_reader :members

      def initialize(members)
        @members = members
      end

      def block?
        false
      end

      def type_compatible?(other)
        @members.any? do |member|
          member.type_compatible?(other)
        end
      end
    end
  end
end
