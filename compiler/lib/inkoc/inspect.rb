# frozen_string_literal: true

module Inkoc
  module Inspect
    def inspect
      names = instance_variables - [:@location]
      pairs = []

      names.each do |name|
        value = instance_variable_get(name)

        next if value.nil? || value.respond_to?(:empty?) && value.empty?

        pairs << "#{name}: #{value.inspect}"
      end

      name = self.class.name.sub('Inkoc::', '')

      if pairs.empty?
        name
      else
        "#{name}(#{pairs.join(', ')})"
      end
    end
  end
end
