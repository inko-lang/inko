# frozen_string_literal: true

module Inkoc
  module Equality
    def ==(other)
      return false unless other.is_a?(self.class)

      ours = instance_variables
      theirs = other.instance_variables

      return false unless ours.length == theirs.length

      ours.zip(theirs).all? do |our, their|
        instance_variable_get(our) == other.instance_variable_get(their)
      end
    end
  end
end
